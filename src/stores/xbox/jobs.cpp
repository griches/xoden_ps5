// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/jobs.h"
#include "platform/random_id.h"
#include "steam/backend/json.h"
#include <algorithm>
#include <set>
#include <sys/stat.h>

namespace playden::stores::xbox {
namespace {
using steam::json::Value;
std::string Text(const Value& value, const char* key) {
    const auto* field = value.Find(key); return field && field->is_string() ? field->text() : std::string{};
}
bool Hash(const std::string& value) {
    return value.size() == 64 && std::all_of(value.begin(),value.end(),[](char c) { return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'); });
}
bool Program(const std::string& value) {
    if (value.empty() || value.size() > 256 || value.front() == '/' || value.find('\\') != std::string::npos || value.find(':') != std::string::npos) return false;
    if (!std::all_of(value.begin(),value.end(),[](unsigned char c) { return c >= 32 && c < 127; })) return false;
    std::size_t at = 0;
    while (at < value.size()) {
        const auto end = value.find('/',at); const auto part = value.substr(at,end == std::string::npos ? end : end-at);
        if (part.empty() || part == "." || part == "..") return false;
        if (end == std::string::npos) break;
        at = end + 1;
    }
    if (value.size() < 4) return false;
    auto extension = value.substr(value.size()-4);
    std::transform(extension.begin(), extension.end(), extension.begin(), [](char c) {
        return c >= 'A' && c <= 'Z' ? static_cast<char>(c + ('a' - 'A')) : c;
    });
    return extension == ".exe";
}
std::optional<Value> Read(Storage& storage, const char* name, std::size_t limit) {
    const auto bytes = storage.Read(name, limit);
    if (!bytes.ok()) return std::nullopt;
    return steam::json::Parse(std::string_view(reinterpret_cast<const char*>(bytes.value().data()),bytes.value().size()),
                              {limit,8,10000,8192});
}
bool Schema(const Value& value) { const auto* schema = value.Find("schema"); return schema && schema->AsUint64() == 1; }
bool WorkerTitle(const std::string& value) {
    return value.size() == 9 && value.compare(0,6,"FAKE78") == 0 && std::all_of(value.begin()+6,value.end(),[](char c) { return c >= '0' && c <= '9'; });
}
}  // namespace

Jobs::Jobs(JobConfig config) : config_(std::move(config)), storage_(config_.root), state_(std::make_shared<JobSnapshot>()), worker_([this] { Run(); }) {}
Jobs::~Jobs() { Shutdown(); }
void Jobs::Shutdown() {
    { std::lock_guard<std::mutex> lock(mutex_); stopping_ = true; pending_.reset(); }
    wake_.notify_all(); if (worker_.joinable()) worker_.join();
}
std::shared_ptr<const JobSnapshot> Jobs::Snapshot() const { std::lock_guard<std::mutex> lock(mutex_); return state_; }
void Jobs::Publish(std::string phase, std::string message) {
    std::lock_guard<std::mutex> lock(mutex_);
    auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision; next->phase = std::move(phase); next->message = std::move(message); state_ = std::move(next);
}
bool Jobs::Install(GameId game, std::string account_key, std::string market) {
    if (!game.valid() || game.store != Store::kXbox || !Hash(account_key) || market.size() != 2 ||
        !std::all_of(market.begin(),market.end(),[](char c) { return c >= 'A' && c <= 'Z'; })) return false;
    std::lock_guard<std::mutex> lock(mutex_);
    if (stopping_ || busy_ || state_->phase == "loading" || state_->phase == "handoff" || state_->worker_title.empty()) return false;
    busy_ = true; pending_ = Request{std::move(game),std::move(account_key),std::move(market)}; wake_.notify_one(); return true;
}
void Jobs::Load() {
    auto identity = config_.hold_effective_uid ? config_.hold_effective_uid() : std::shared_ptr<void>{};
    auto next = std::make_shared<JobSnapshot>(); next->revision = 1; next->phase = "idle";
    if (const auto worker = Read(storage_,"worker.json",4096); worker && Schema(*worker)) {
        const auto id = ParseInstallId(Text(*worker,"installId")); const auto title = Text(*worker,"titleId");
        if (id && id->store == Store::kLocal && WorkerTitle(title)) { next->worker_install_id = ToInstallId(*id); next->worker_title = title; }
    }
    if (const auto prepared = Read(storage_,"prepared-games.json",2*1024*1024); prepared && Schema(*prepared)) {
        const auto* rows = prepared->Find("games");
        if (rows && rows->is_array() && rows->array().size() <= 400) {
            std::set<GameId> seen;
            bool valid = true;
            for (const auto& row : rows->array()) {
                const auto game = GameId::Xbox(Text(row,"productId")); const auto account = Text(row,"accountKey");
                const auto program = Text(row,"executable"); const auto folder = Text(row,"folder");
                if (!game.valid() || !Hash(account) || !Program(program) || folder != ToInstallId(game) || !seen.insert(game).second) { valid = false; break; }
                const auto root = config_.games_root + "/" + folder;
                struct stat st{};
                if (lstat(root.c_str(),&st) == 0 && S_ISDIR(st.st_mode)) next->prepared.push_back({game,account,program,root});
            }
            if (!valid) { next->prepared.clear(); next->message = "Xbox's prepared-game list could not be read."; }
        }
    }
    const auto request = Read(storage_,"job-request.json",4096);
    const auto result = Read(storage_,"job-result.json",1024*1024);
    if (request && result && Schema(*request) && Schema(*result) && Text(*request,"requestId") == Text(*result,"requestId")) {
        const auto phase = Text(*result,"status");
        if (phase == "failed") { next->phase = "failed"; next->message = "The Xbox download did not finish. Choose Install to retry."; }
        else if (phase == "package-choice") { next->phase = "failed"; next->message = "This offer has multiple PC packages. Automatic selection is unavailable."; }
        else if (phase != "prepared" && phase != "complete") { next->phase = "failed"; next->message = "The Xbox operation was interrupted. Choose Install to resume it."; }
    }
    std::lock_guard<std::mutex> lock(mutex_); state_ = std::move(next);
}
void Jobs::Execute(const Request& request) {
    auto identity = config_.hold_effective_uid ? config_.hold_effective_uid() : std::shared_ptr<void>{};
    Publish("starting", "Starting the Xbox download service");
    const auto id = RandomUuid();
    if (!id) { Publish("failed","A download request could not be created."); return; }
    // Every interpolated value has a strict ASCII identity grammar.
    const auto text = "{\"schema\":1,\"requestId\":\"" + *id + "\",\"operation\":\"install\",\"market\":\"" + request.market +
        "\",\"accountKey\":\"" + request.account + "\",\"productId\":\"" + request.game.value + "\"}";
    if (!storage_.WriteAtomicPrivate("job-request.json",Bytes(text.begin(),text.end())).ok()) { Publish("failed","The Xbox download request could not be saved."); return; }
    const auto worker = Snapshot()->worker_title;
    std::string error;
    if (!config_.handoff || !config_.handoff(*id,worker,&error)) { Publish("failed",error.empty() ? "The Xbox worker could not be started." : error); return; }
    {
        std::lock_guard<std::mutex> lock(mutex_);
        auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision; next->phase = "handoff"; next->request_id = *id;
        next->message = "Opening the Xbox download screen"; state_ = std::move(next);
    }
}
void Jobs::Run() {
    Load();
    for (;;) {
        Request request;
        { std::unique_lock<std::mutex> lock(mutex_); wake_.wait(lock,[this] { return stopping_ || pending_.has_value(); });
          if (stopping_) return; request = std::move(*pending_); pending_.reset(); }
        Execute(request);
        std::lock_guard<std::mutex> lock(mutex_); busy_ = false;
    }
}
}  // namespace playden::stores::xbox
