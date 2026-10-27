// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/jobs.h"
#include "platform/random_id.h"
#include "platform/managed_directory.h"
#include <dirent.h>
#include <unistd.h>
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
std::string JsonQuote(const std::string& text) {
    std::string out = "\"";
    const char* hex = "0123456789abcdef";
    for (unsigned char c : text) {
        if (c == '"' || c == '\\') { out += '\\'; out += c; }
        else if (c < 32) { out += "\\u00"; out += hex[c >> 4]; out += hex[c & 15]; }
        else out += c;
    }
    return out + "\"";
}
std::string Encode(const Value& v) {
    if (v.is_string()) return JsonQuote(v.text());
    if (v.is_number()) return v.text();
    if (v.is_bool()) return v.boolean() ? "true" : "false";
    if (v.is_null()) return "null";
    std::string out = v.is_array() ? "[" : "{"; bool first = true;
    if (v.is_array()) for (const auto& value : v.array()) { if (!first) out += ','; first = false; out += Encode(value); }
    else for (const auto& [key,value] : v.object()) { if (!first) out += ','; first = false; out += JsonQuote(key) + ':' + Encode(value); }
    return out + (v.is_array() ? "]" : "}");
}
bool ContentId(const std::string& value) {
    if (value.size() != 36) return false;
    for (std::size_t i = 0; i < value.size(); ++i) {
        if (i == 8 || i == 13 || i == 18 || i == 23) { if (value[i] != '-') return false; }
        else if (!((value[i] >= '0' && value[i] <= '9') || (value[i] >= 'a' && value[i] <= 'f'))) return false;
    }
    return true;
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
// Managed directories reject symlink components; removal never follows links.
// Keep the resume marker until the rest is deleted so failures remain retryable.
bool RemovePartial(platform::ManagedDirectory& directory, unsigned depth = 0) {
    if (depth > 64 || !directory.Validate()) return false;
    DIR* listing = opendir(directory.path().c_str());
    if (!listing) return false;
    std::vector<std::string> names;
    errno = 0;
    while (const auto* entry = readdir(listing)) {
        const std::string name = entry->d_name;
        if (name != "." && name != "..") names.push_back(name);
    }
    const bool read_ok = errno == 0;
    closedir(listing);
    if (!read_ok) return false;
    std::stable_sort(names.begin(), names.end(), [](const auto& a, const auto& b) {
        return (a == ".nexus-streaming-v1.jsonl" || a == ".xodus-streaming-tmp.msixvc") <
               (b == ".nexus-streaming-v1.jsonl" || b == ".xodus-streaming-tmp.msixvc");
    });
    for (const auto& name : names) {
        struct stat st{};
        if (!directory.InspectChild(name, &st)) return false;
        if (S_ISDIR(st.st_mode)) {
            auto child = directory.OpenChild(name, platform::DirectoryPolicy::kApprovedTree);
            if (!child || !RemovePartial(*child, depth + 1) || !directory.RemoveEmptyDirectory(name)) return false;
        } else if (!S_ISREG(st.st_mode) || !directory.RemoveRegularFile(name)) return false;
    }
    return directory.Sync();
}
bool PreparedManifest(Storage& storage, std::set<GameId>* games) {
    const auto manifest = Read(storage,"prepared-games.json",2*1024*1024);
    if (!manifest || !Schema(*manifest)) return false;
    const auto* rows = manifest->Find("games");
    if (!rows || !rows->is_array() || rows->array().size() > 400) return false;
    for (const auto& row : rows->array()) {
        const auto game = GameId::Xbox(Text(row,"productId"));
        if (!game.valid() || Text(row,"folder") != ToInstallId(game)) return false;
        games->insert(game);
    }
    return true;
}
std::vector<GameId> PartialGames(Storage& storage, const std::string& root) {
    std::set<GameId> prepared;
    std::optional<GameId> first_download;
    if (!PreparedManifest(storage, &prepared)) {
        // Before the first successful install there is no prepared manifest.
        // Admit only the current, explicitly unfinished install in that case.
        if (storage.Read("prepared-games.json").error() != StorageError::kNotFound) return {};
        const auto request = Read(storage,"job-request.json",4096);
        const auto result = Read(storage,"job-result.json",1024*1024);
        if (!request || !result || !Schema(*request) || !Schema(*result) ||
            Text(*request,"operation") != "install" || Text(*request,"requestId").empty() ||
            Text(*request,"requestId") != Text(*result,"requestId") ||
            Text(*request,"productId") != Text(*result,"productId")) return {};
        const auto phase = Text(*result,"status");
        if (phase != "downloading" && phase != "failed" && phase != "cancelled") return {};
        const auto game = GameId::Xbox(Text(*request,"productId"));
        if (!game.valid()) return {};
        first_download = game;
    }
    auto parent = platform::ManagedDirectory::Open(root,platform::DirectoryPolicy::kApprovedTree);
    if (!parent) return {};
    DIR* listing = opendir(root.c_str());
    if (!listing) return {};
    std::vector<GameId> games;
    while (const auto* entry = readdir(listing)) {
        const std::string name = entry->d_name;
        const bool deleting = name.rfind(".delete-xbox-",0) == 0;
        const auto game = ParseInstallId(deleting ? name.substr(8) : name);
        if (!game || game->store != Store::kXbox || ToInstallId(*game) != (deleting ? name.substr(8) : name) || prepared.count(*game) ||
            (first_download && *first_download != *game)) continue;
        auto child = parent->OpenChild(name,platform::DirectoryPolicy::kApprovedTree);
        struct stat marker{};
        if (child && (deleting || (child->InspectChild(".nexus-streaming-v1.jsonl", &marker) && S_ISREG(marker.st_mode)) ||
                      (child->InspectChild(".playden-xbox-uninstall", &marker) && S_ISREG(marker.st_mode)) ||
                      (child->InspectChild(".xodus-streaming-tmp.msixvc", &marker) && S_ISREG(marker.st_mode)))) games.push_back(*game);
    }
    closedir(listing);
    std::sort(games.begin(),games.end());
    games.erase(std::unique(games.begin(),games.end()),games.end());
    return games;
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
bool Jobs::Install(GameId game, std::string account_key, std::string market, std::string content_id) {
    if ((!content_id.empty() && !ContentId(content_id)) || !game.valid() || game.store != Store::kXbox || !Hash(account_key) || market.size() != 2 ||
        !std::all_of(market.begin(),market.end(),[](char c) { return c >= 'A' && c <= 'Z'; })) return false;
    std::lock_guard<std::mutex> lock(mutex_);
    if (stopping_ || busy_ || state_->phase == "loading" || state_->phase == "handoff" || (!config_.native_run && state_->worker_title.empty())) return false;
    cancel_requested_ = pause_requested_ = false;
    if (config_.native_run) {
        auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
        next->choices.clear(); next->choice_game = {};
        next->phone_test=false;next->browser_signin=false;next->sign_in_code.clear();next->sign_in_url.clear();
        next->active = true; next->cancelling = false; next->active_game = game;
        next->done_bytes = next->total_bytes = next->bytes_per_second = 0;
        next->phase = "starting"; next->message = "Starting Xbox download…"; state_ = std::move(next);
    }
    busy_ = true; pending_ = Request{std::move(game),std::move(account_key),std::move(market),false,false,std::move(content_id)}; wake_.notify_one(); return true;
}
bool Jobs::Refresh(std::string market) {
    if (market.size() != 2 || !std::all_of(market.begin(),market.end(),[](char c) { return c >= 'A' && c <= 'Z'; })) return false;
    std::lock_guard<std::mutex> lock(mutex_);
    if (!config_.native_run || stopping_ || busy_ || state_->phase == "loading") return false;
    cancel_requested_ = pause_requested_ = false;
    auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
    next->phone_test=false;next->browser_signin=false;next->sign_in_code.clear();next->sign_in_url.clear();
    next->active = true; next->active_game = {}; next->phase = "refreshing";
    next->message = "Checking Xbox account and refreshing library…"; state_ = std::move(next);
    busy_ = true; pending_ = Request{{},{},std::move(market),false,false,{},true}; wake_.notify_one(); return true;
}
bool Jobs::Cancel(GameId game) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!config_.native_run || !busy_ || !state_->active || state_->active_game != game || cancel_requested_ || pause_requested_) return false;
    cancel_requested_ = true;
    auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
    next->cancelling = true; next->phase = "stopping";
    next->sign_in_code.clear(); next->sign_in_url.clear();
    next->message = next->phone_test ? "Cancelling phone sign-in…" : !next->active_game.valid() ? "Cancelling account operation…" : "Stopping download before deleting files…";
    state_ = std::move(next); return true;
}
bool Jobs::TestPhoneSignIn(GameId game, std::string market) {
    if (!game.valid() || game.store != Store::kXbox || market.size()!=2 ||
        !std::all_of(market.begin(),market.end(),[](char c) {return c>='A' && c<='Z';})) return false;
    std::lock_guard<std::mutex> lock(mutex_);
    if (!config_.native_run || stopping_ || busy_ || state_->phase == "loading") return false;
    cancel_requested_ = pause_requested_ = false;
    auto next=std::make_shared<JobSnapshot>(*state_); ++next->revision;
    next->active=true; next->active_game={}; next->phone_test=true;next->browser_signin=false; next->cancelling=false;
    next->sign_in_code.clear();next->sign_in_url.clear();next->phase="checking-sign-in";
    next->message="Preparing phone sign-in without changing your current account…";
    std::string content;
    for(const auto& prepared:state_->prepared) if(prepared.game==game) content=prepared.content_id;
    state_=std::move(next);busy_=true;pending_=Request{game,{},std::move(market),false,false,std::move(content),false,true};wake_.notify_one();return true;
}
bool Jobs::SignOut(std::string market) {
    if (market.size()!=2 || !std::all_of(market.begin(),market.end(),[](char c){return c>='A'&&c<='Z';})) return false;
    std::lock_guard<std::mutex> lock(mutex_);
    if (!config_.native_run || stopping_ || busy_ || state_->phase=="loading") return false;
    cancel_requested_=pause_requested_=false;
    auto next=std::make_shared<JobSnapshot>(*state_);++next->revision;
    next->active=true;next->active_game={};next->phone_test=false;next->browser_signin=false;next->sign_in_code.clear();next->sign_in_url.clear();
    next->phase="signing-out";next->message="Clearing the saved account…";state_=std::move(next);
    busy_=true;pending_=Request{{},{},std::move(market),false,false,{},false,false,true};wake_.notify_one();return true;
}
bool Jobs::BrowserSignIn(std::string market) {
    if (market.size()!=2 || !std::all_of(market.begin(),market.end(),[](char c){return c>='A'&&c<='Z';})) return false;
    std::lock_guard<std::mutex> lock(mutex_);
    if (!config_.native_run || stopping_ || busy_ || state_->phase=="loading") return false;
    cancel_requested_=pause_requested_=false;
    auto next=std::make_shared<JobSnapshot>(*state_);++next->revision;
    next->active=true;next->active_game={};next->phone_test=false;next->browser_signin=true;next->cancelling=false;
    next->sign_in_code.clear();next->sign_in_url.clear();next->phase="checking-sign-in";
    next->message="Preparing Microsoft sign-in…";state_=std::move(next);
    busy_=true;pending_=Request{{},{},std::move(market),false,false,{},false,false,false,true};wake_.notify_one();return true;
}
bool Jobs::Pause(GameId game) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!config_.native_run || !busy_ || !state_->active || state_->active_game != game || cancel_requested_ || pause_requested_) return false;
    pause_requested_ = true;
    auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
    next->cancelling = true; next->phase = "stopping"; next->message = "Pausing download; keeping downloaded files…";
    state_ = std::move(next); return true;
}
bool Jobs::Discard(GameId game) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (stopping_ || busy_ || state_->phase == "loading" || state_->phase == "handoff" ||
        std::find(state_->partials.begin(),state_->partials.end(),game) == state_->partials.end()) return false;
    busy_ = true; pending_ = Request{game,{}, {},true}; wake_.notify_one(); return true;
}
bool Jobs::Uninstall(GameId game) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (stopping_ || busy_ || state_->phase == "loading" || state_->phase == "handoff" ||
        std::none_of(state_->prepared.begin(),state_->prepared.end(),[&](const auto& p) { return p.game == game; })) return false;
    auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
    next->removing_game = game; next->removal_error.clear(); next->phase = "uninstalling";
    state_ = std::move(next); busy_ = true; pending_ = Request{game,{},{},false,true}; wake_.notify_one(); return true;
}
void Jobs::ExecuteUninstall(GameId game) {
    auto identity = config_.hold_effective_uid ? config_.hold_effective_uid() : std::shared_ptr<void>{};
    const auto fail = [this,game](const std::string& message) {
        std::lock_guard<std::mutex> lock(mutex_);
        auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
        next->removing_game = game; next->removal_error = message; next->phase = "failed"; next->message = message;
        state_ = std::move(next);
    };
    const auto manifest = Read(storage_,"prepared-games.json",2*1024*1024);
    std::set<GameId> prepared;
    if (!manifest || !PreparedManifest(storage_,&prepared) || !prepared.count(game)) { fail("Xbox install metadata could not be verified. Files were kept."); return; }
    std::string updated = "{\"schema\":1,\"games\":["; bool first = true;
    for (const auto& row : manifest->Find("games")->array()) {
        if (!Hash(Text(row,"accountKey")) || !Program(Text(row,"executable"))) { fail("Xbox install metadata is invalid. Files were kept."); return; }
        if (Text(row,"productId") == game.value) continue;
        if (!first) updated += ','; first = false; updated += Encode(row);
    }
    updated += "]}";
    const auto root = config_.games_root + "/" + ToInstallId(game);
    auto directory = platform::ManagedDirectory::Open(root,platform::DirectoryPolicy::kApprovedTree);
    if (!directory) { fail("The Xbox game folder could not be verified. Files were kept."); return; }
    // Durable intent precedes manifest removal. A crash leaves a recognisable
    // partial for managed cleanup, even when the download journal is absent.
    Storage content(root);
    const std::string intent = "Xbox uninstall requested after title removal\n";
    if (!content.WriteAtomicPrivate(".playden-xbox-uninstall",Bytes(intent.begin(),intent.end())).ok() ||
        !storage_.WriteAtomicPrivate("prepared-games.json",Bytes(updated.begin(),updated.end())).ok()) {
        fail("The Xbox uninstall could not be saved. Files were kept."); return;
    }
    ExecuteDiscard(game);
    const auto outcome = Snapshot();
    if (outcome->phase != "cleaned") { Load(); fail(outcome->message); return; }
    Load(); Publish("cleaned","Xbox game uninstalled and downloaded files deleted.");
}
void Jobs::ExecuteDiscard(GameId game) {
    auto identity = config_.hold_effective_uid ? config_.hold_effective_uid() : std::shared_ptr<void>{};
    Publish("cleaning","Deleting partial Xbox download…");
    // Reload admission on the service worker immediately before touching files.
    const auto partials = PartialGames(storage_,config_.games_root);
    if (std::find(partials.begin(),partials.end(),game) == partials.end()) {
        Publish("failed","This download can no longer be safely removed."); return;
    }
    auto parent = platform::ManagedDirectory::Open(config_.games_root,platform::DirectoryPolicy::kApprovedTree);
    if (!parent) { Publish("failed","The download folder could not be opened."); return; }
    const auto folder = ToInstallId(game);
    const auto quarantine = ".delete-" + folder;
    auto child = parent->OpenChild(quarantine,platform::DirectoryPolicy::kApprovedTree);
    if (child) {
        struct stat current{};
        if (parent->InspectChild(folder,&current) || errno != ENOENT) {
            Publish("failed","Both a new download and an unfinished deletion exist. Files were kept."); return;
        }
    }
    if (!child) {
        if (errno != ENOENT || !parent->RenameChild(folder,*parent,quarantine,false) || !parent->Sync()) {
            Publish("failed","The partial download could not be set aside for deletion."); return;
        }
        child = parent->OpenChild(quarantine,platform::DirectoryPolicy::kApprovedTree);
    }
    // The canonical quarantine name persists even after the last resume marker
    // has gone. A restart can retry cleanup without ever touching a new install.
    if (!child || !RemovePartial(*child)) {
        Publish("failed","Some download files could not be deleted. Try again."); return;
    }
    const auto request = Read(storage_,"job-request.json",4096);
    if (request && Schema(*request) && Text(*request,"operation") == "install" && Text(*request,"productId") == game.value) {
        const auto id = Text(*request,"requestId");
        const bool safe = id.size() == 36 && std::all_of(id.begin(),id.end(),[](char c) {
            return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || c == '-';
        });
        if (safe) {
            const auto text = "{\"schema\":1,\"requestId\":\"" + id + "\",\"operation\":\"install\",\"productId\":\"" +
                game.value + "\",\"status\":\"cancelled\",\"detail\":\"Partial download deleted\"}";
            if (!storage_.WriteAtomicPrivate("job-result.json",Bytes(text.begin(),text.end())).ok()) {
                Publish("failed","Download files deleted, but the cancellation status could not be saved."); return;
            }
        }
    }
    if (!parent->RemoveEmptyDirectory(quarantine) || !parent->Sync()) {
        Publish("failed","Download files deleted; retry to finish the folder cleanup."); return;
    }
    {
        std::lock_guard<std::mutex> lock(mutex_);
        auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
        next->partials.erase(std::remove(next->partials.begin(),next->partials.end(),game),next->partials.end());
        // Completion must never expose the just-removed game as installed,
        // including the intermediate snapshot consumed by ExecuteUninstall.
        next->prepared.erase(std::remove_if(next->prepared.begin(),next->prepared.end(),
            [&](const auto& item) { return item.game == game; }),next->prepared.end());
        next->phase = "cleaned"; next->message = "Partial Xbox download deleted."; state_ = std::move(next);
    }
}
void Jobs::Load() {
    auto identity = config_.hold_effective_uid ? config_.hold_effective_uid() : std::shared_ptr<void>{};
    auto next = std::make_shared<JobSnapshot>(); next->revision = Snapshot()->revision + 1; next->phase = "idle"; next->native = bool(config_.native_run);
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
                const auto program = Text(row,"executable"); const auto folder = Text(row,"folder");const auto content=Text(row,"contentId");
                if (!game.valid() || !Hash(account) || !Program(program) || folder != ToInstallId(game) || !seen.insert(game).second) { valid = false; break; }
                const auto root = config_.games_root + "/" + folder;
                struct stat st{};
                if (lstat(root.c_str(),&st) == 0 && S_ISDIR(st.st_mode)) next->prepared.push_back({game,account,program,root,ContentId(content)?content:std::string{}});
            }
            if (!valid) { next->prepared.clear(); next->message = "Xbox's prepared-game list could not be read."; }
        }
    }
    next->partials = PartialGames(storage_,config_.games_root);
    const auto request = Read(storage_,"job-request.json",4096);
    const auto result = Read(storage_,"job-result.json",1024*1024);
    if (request && result && Schema(*request) && Schema(*result) && Text(*request,"requestId") == Text(*result,"requestId")) {
        const auto phase = Text(*result,"status");
        next->active_game = GameId::Xbox(Text(*request,"productId"));
        next->phone_test = Text(*request,"operation") == "signin-test";
        next->browser_signin = Text(*request,"operation") == "signin-browser";
        if (next->browser_signin) {
            next->active_game={};next->request_id=Text(*request,"requestId");
            const auto* detail=result->Find("detail");
            const auto* matched=detail?detail->Find("browserSessionSaved"):nullptr;
            if (phase=="browser-sign-in-saved" && matched && matched->is_bool() && matched->boolean()) {
                next->phase=phase;next->message="Microsoft sign-in saved. Loading your library…";
            } else {
                next->phase=phase=="cancelled"?"cancelled":"failed";
                const auto code=detail?Text(*detail,"code"):std::string{};
                next->message=phase=="cancelled"?"Sign-in cancelled. Your saved account and games were kept.":
                    code=="browser-account-mismatch"?"That Microsoft account does not match your PS5 account. Try again with the same account.":
                    code=="phone-sign-in-required"?"Sign in with Microsoft to connect your account.":
                    code=="browser-sign-in-expired"?"Sign-in expired. Try again for a new pairing code.":
                    "Microsoft sign-in did not finish. Your saved account was kept; try again.";
            }
        } else if (next->phone_test) {
            next->active_game={}; next->request_id=Text(*request,"requestId");
            const auto* detail=result->Find("detail");
            const auto* licensed=detail?detail->Find("pcLicenseVerified"):nullptr;
            const auto* replaced=detail?detail->Find("activeAccountReplaced"):nullptr;
            if (phase=="phone-sign-in-verified" && licensed && licensed->is_bool() && licensed->boolean() &&
                replaced && replaced->is_bool() && !replaced->boolean()) {
                next->phase=phase; next->message="Phone sign-in and PC license verified. Your current account is unchanged. Game sign-in is still being tested.";
            } else {
                next->phase=phase=="cancelled"?"cancelled":"failed";
                const auto code=detail?Text(*detail,"code"):std::string{};
                next->message=phase=="cancelled"?"Phone sign-in cancelled. Your current account is unchanged.":
                    code=="phone-code-expired"?"The sign-in code expired. Request a new code.":
                    code=="phone-sign-in-denied"?"Microsoft sign-in was declined. Your current account is unchanged.":
                    code=="phone-pc-license"?"Phone sign-in did not obtain this game's PC license. Your current account is unchanged.":
                    "Phone sign-in did not finish. Your current account is unchanged; try again.";
            }
        } else if (phase=="signed-out" && Text(*request,"operation")=="signout") {
            next->active_game={};next->request_id=Text(*request,"requestId");next->phase="signed-out";
            next->message="Signed out. The saved account and cached library were cleared. Installed games and saves are kept.";
        } else if (phase == "failed") {
            next->phase = "failed"; next->message = "The Xbox download did not finish. Resume to retry; downloaded files were kept.";
            const auto* detail = result->Find("detail");
            const auto code = detail ? Text(*detail,"code") : std::string{};
            if (code == "sign-in-required") next->message = "Sign in to Xbox before downloading. Your saved sign-in is missing or expired.";
            else if (code == "account-changed") next->message = "The Xbox account changed. Refresh your Xbox library before downloading.";
            else if (code == "active-pc-subscription-required") next->message = "An active PC Game Pass or Game Pass Ultimate subscription is required.";
            else if (code == "game-entitlement-required") next->message = "This Xbox account does not currently have access to this game.";
            else if (code == "metadata-memory") next->message = "The app ran out of working memory while reading this package. Downloaded files were kept.";
            else if (code == "metadata-invalid") next->message = "The game's package data could not be read. Retry the download; existing files were kept.";
            else if (code == "package-resolution") next->message = "The Store could not find this game's PC packages. Try again.";
        }
        else if (phase == "package-choice") {
            next->phase = "package-choice"; next->choice_game = GameId::Xbox(Text(*request,"productId"));
            const auto* detail = result->Find("detail");
            const auto* choices = detail ? detail->Find("choices") : nullptr;
            std::set<std::string> seen;
            if (next->choice_game.valid() && detail && Text(*detail,"rootProductId") == next->choice_game.value &&
                Text(*detail,"market") == Text(*request,"market") && choices && choices->is_array() && choices->array().size() <= 128) {
                for (const auto& choice : choices->array()) {
                    const auto cid = Text(choice,"contentId"), title = Text(choice,"title");
                    const auto* supported = choice.Find("supported");
                    if (!supported || !supported->is_bool() || !supported->boolean() || !ContentId(cid) || title.empty() || title.size() > 1024 ||
                        Text(choice,"platform") != "Windows.Desktop" || Text(choice,"packageFormat") != "MSIXVC" || !seen.insert(cid).second) continue;
                    const auto* bytes = choice.Find("downloadBytes");
                    next->choices.push_back({cid,title,bytes ? bytes->AsUint64().value_or(0) : 0});
                }
            }
            next->message = next->choices.empty() ? "This offer has no supported PC packages." : "Choose a PC package to install.";
        }
        else if (phase == "complete" && Text(*request,"operation") == "refresh") { next->phase = "refreshed"; next->request_id = Text(*request,"requestId"); next->message = "Xbox account checked and library refreshed."; }
        else if (phase == "cancelled" && !next->partials.empty()) { next->phase = "cancelled"; next->message = "Download paused. Resume to continue, or cancel to delete its files."; }
        else if (phase != "prepared" && phase != "complete" && phase != "cancelled") { next->phase = "failed"; next->message = "The Xbox operation was interrupted. Choose Install to resume it."; }
    }
    if (std::find(next->partials.begin(),next->partials.end(),next->active_game) != next->partials.end()) {
        const auto checkpoint = Read(storage_,("progress-" + ToInstallId(next->active_game) + ".json").c_str(),4096);
        if (checkpoint && Schema(*checkpoint) && Text(*checkpoint,"productId") == next->active_game.value) {
            const auto* done = checkpoint->Find("doneBytes"); const auto* total = checkpoint->Find("totalBytes");
            if (done && total && done->AsUint64() && total->AsUint64() && *done->AsUint64() <= *total->AsUint64()) {
                next->done_bytes = *done->AsUint64(); next->total_bytes = *total->AsUint64();
            }
        }
    }
    std::lock_guard<std::mutex> lock(mutex_); state_ = std::move(next);
}
void Jobs::Execute(const Request& request) {
    auto identity = config_.hold_effective_uid ? config_.hold_effective_uid() : std::shared_ptr<void>{};
    if (request.browser_signin) {
        const auto id=RandomUuid();if(!id){Publish("failed","A computer sign-in request could not be created.");return;}
        const auto text="{\"schema\":1,\"requestId\":\""+*id+"\",\"operation\":\"signin-browser\",\"market\":\""+request.market+"\"}";
        if(!storage_.WriteAtomicPrivate("job-request.json",Bytes(text.begin(),text.end())).ok()){Publish("failed","The computer sign-in request could not be saved.");return;}
        ExecuteNative({},*id);return;
    }
    if (request.signout) {
        const auto id=RandomUuid();if(!id){Publish("failed","A sign-out request could not be created.");return;}
        const auto text="{\"schema\":1,\"requestId\":\""+*id+"\",\"operation\":\"signout\",\"market\":\""+request.market+"\"}";
        if(!storage_.WriteAtomicPrivate("job-request.json",Bytes(text.begin(),text.end())).ok()){Publish("failed","The sign-out request could not be saved.");return;}
        ExecuteNative({},*id);return;
    }
    if (request.phone_test) {
        const auto id=RandomUuid();
        if (!id) {Publish("failed","A phone sign-in request could not be created.");return;}
        const auto text="{\"schema\":1,\"requestId\":\""+*id+"\",\"operation\":\"signin-test\",\"market\":\""+request.market+
            "\",\"productId\":\""+request.game.value+"\""+(request.content.empty()?"":",\"contentId\":\""+request.content+"\"")+"}";
        if (!storage_.WriteAtomicPrivate("job-request.json",Bytes(text.begin(),text.end())).ok()) {
            Publish("failed","The phone sign-in request could not be saved.");return;
        }
        ExecuteNative({},*id);return;
    }
    if (request.refresh) {
        const auto id = RandomUuid();
        if (!id) { Publish("failed","An Xbox account request could not be created."); return; }
        const auto text = "{\"schema\":1,\"requestId\":\"" + *id + "\",\"operation\":\"refresh\",\"market\":\"" + request.market + "\"}";
        if (!storage_.WriteAtomicPrivate("job-request.json",Bytes(text.begin(),text.end())).ok()) { Publish("failed","The Xbox account request could not be saved."); return; }
        ExecuteNative({},*id); return;
    }
    struct stat pending{};
    const auto quarantine = config_.games_root + "/.delete-" + ToInstallId(request.game);
    if (lstat(quarantine.c_str(),&pending) == 0 || errno != ENOENT) {
        Publish("failed","Finish deleting this game's partial download before installing it again."); return;
    }
    Publish("starting", "Starting the Xbox download service");
    const auto id = RandomUuid();
    if (!id) { Publish("failed","A download request could not be created."); return; }
    // Every interpolated value has a strict ASCII identity grammar.
    std::string content = request.content;
    if (content.empty()) {
        const auto previous = Read(storage_,("selection-" + ToInstallId(request.game) + ".json").c_str(),4096);
        if (previous && Schema(*previous) && Text(*previous,"productId") == request.game.value &&
            Text(*previous,"accountKey") == request.account && Text(*previous,"market") == request.market &&
            ContentId(Text(*previous,"contentId"))) content = Text(*previous,"contentId");
    }
    const auto snapshot = Snapshot();
    const std::string operation = config_.native_run && content.empty() &&
        std::find(snapshot->partials.begin(),snapshot->partials.end(),request.game) == snapshot->partials.end() ? "plan" : "install";
    const auto text = "{\"schema\":1,\"requestId\":\"" + *id + "\",\"operation\":\"" + operation + "\",\"market\":\"" + request.market +
        "\",\"accountKey\":\"" + request.account + "\",\"productId\":\"" + request.game.value + "\"" + (content.empty() ? "" : ",\"contentId\":\"" + content + "\"") + "}";
    if (!content.empty() && !storage_.WriteAtomicPrivate("selection-" + ToInstallId(request.game) + ".json",Bytes(text.begin(),text.end())).ok()) {
        Publish("failed","The selected Xbox package could not be saved. Download has not started."); return;
    }
    if (!storage_.WriteAtomicPrivate("job-request.json",Bytes(text.begin(),text.end())).ok()) { Publish("failed","The Xbox download request could not be saved."); return; }
    if (config_.native_run) { ExecuteNative(request.game,*id); return; }
    const auto worker = Snapshot()->worker_title;
    std::string error;
    if (!config_.handoff || !config_.handoff(*id,worker,&error)) { Publish("failed",error.empty() ? "The Xbox worker could not be started." : error); return; }
    {
        std::lock_guard<std::mutex> lock(mutex_);
        auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision; next->phase = "handoff"; next->request_id = *id;
        next->message = "Opening the Xbox download screen"; state_ = std::move(next);
    }
}
void Jobs::ExecuteNative(GameId game, const std::string& request_id) {
    const auto result = config_.native_run([this,&request_id](const NativeProgress& progress) {
        std::string code,url;
        if (progress.phase=="sign-in-code") {
            const auto record=Read(storage_,"job-result.json",8192);
            const auto* detail=record?record->Find("detail"):nullptr;
            if (record && Schema(*record) && Text(*record,"requestId")==request_id && Text(*record,"operation")=="signin-test" &&
                Text(*record,"status")=="sign-in-code" && detail) {
                const auto c=Text(*detail,"userCode"),u=Text(*detail,"verificationUri");
                if (!c.empty() && c.size()<=32 && std::all_of(c.begin(),c.end(),[](unsigned char ch){return (ch>='A'&&ch<='Z')||(ch>='a'&&ch<='z')||(ch>='0'&&ch<='9')||ch=='-';}) &&
                    (u=="https://www.microsoft.com/link" || u=="https://microsoft.com/link")) {code=c;url=u;}
            }
        }
        if (progress.phase=="browser-pairing") {
            const auto record=Read(storage_,"job-result.json",8192);
            const auto* detail=record?record->Find("detail"):nullptr;
            if(record && Schema(*record) && Text(*record,"requestId")==request_id && Text(*record,"operation")=="signin-browser" &&
                Text(*record,"status")=="browser-pairing" && detail && Hash(Text(*detail,"fingerprint")))
                code=Text(*detail,"fingerprint").substr(0,16);
        }
        std::lock_guard<std::mutex> lock(mutex_);
        auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
        if (!next->cancelling) { next->phase = progress.phase; next->message = progress.message; }
        if (!next->cancelling) { next->sign_in_code=std::move(code);next->sign_in_url=std::move(url); }
        next->done_bytes = progress.done_bytes; next->total_bytes = progress.total_bytes;
        next->bytes_per_second = progress.bytes_per_second; state_ = std::move(next);
    },[this] { std::lock_guard<std::mutex> lock(mutex_); return stopping_ || cancel_requested_ || pause_requested_; });
    bool discard = false;
    { std::lock_guard<std::mutex> lock(mutex_); discard = cancel_requested_; }
    // The runner's contract guarantees no outstanding file writes here. Reload
    // prepared/partial admission before any cleanup, including late cancellation.
    const auto previous = Snapshot();
    if (game.valid() && result != NativeResult::Complete && previous->total_bytes && previous->done_bytes <= previous->total_bytes) {
        const auto checkpoint = "{\"schema\":1,\"productId\":\"" + game.value + "\",\"doneBytes\":" +
            std::to_string(previous->done_bytes) + ",\"totalBytes\":" + std::to_string(previous->total_bytes) + "}";
        storage_.WriteAtomicPrivate("progress-" + ToInstallId(game) + ".json",Bytes(checkpoint.begin(),checkpoint.end()));
    }
    Load();
    const auto loaded = Snapshot();
    if (loaded->phone_test) {
        if (discard || result==NativeResult::Cancelled) Publish("cancelled","Phone sign-in cancelled. Your current account is unchanged.");
        else if (result!=NativeResult::Complete && loaded->phase!="failed") Publish("failed","Phone sign-in did not finish. Your current account is unchanged.");
        return;
    }
    if (!game.valid()) {
        if(discard || result==NativeResult::Cancelled) Publish("cancelled","Account operation cancelled. Installed games and saves were kept.");
        else if(result==NativeResult::Failed && loaded->phase!="failed") Publish("failed","The account operation did not finish. Try again.");
        return;
    }
    if (std::find(loaded->partials.begin(),loaded->partials.end(),game) != loaded->partials.end()) {
        std::lock_guard<std::mutex> lock(mutex_);
        auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
        next->active_game = game; next->done_bytes = previous->done_bytes; next->total_bytes = previous->total_bytes;
        state_ = std::move(next);
    }
    if (result == NativeResult::Complete) {
        std::lock_guard<std::mutex> lock(mutex_);
        if (std::any_of(state_->prepared.begin(),state_->prepared.end(),[&](const auto& entry) { return entry.game == game; })) {
            auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
            next->completed_game = game; next->request_id = request_id; state_ = std::move(next);
        }
    }
    if (discard) {
        const auto snapshot = Snapshot();
        if (std::find(snapshot->partials.begin(),snapshot->partials.end(),game) != snapshot->partials.end()) ExecuteDiscard(game);
        else if (std::none_of(snapshot->prepared.begin(),snapshot->prepared.end(),[&](const auto& entry) { return entry.game == game; }))
            Publish("cleaned","Xbox download cancelled; no partial files remain.");
    } else if (result == NativeResult::Failed && Snapshot()->phase != "failed") {
        Publish("failed","The Xbox download could not finish. Its partial files were kept.");
    } else if (result == NativeResult::Cancelled) {
        Publish("cancelled","Xbox download stopped. Choose Resume to continue.");
    }
}
void Jobs::Run() {
    Load();
    for (;;) {
        Request request;
        { std::unique_lock<std::mutex> lock(mutex_); wake_.wait(lock,[this] { return stopping_ || pending_.has_value(); });
          if (stopping_) return; request = std::move(*pending_); pending_.reset(); }
        if (request.uninstall) ExecuteUninstall(request.game); else if (request.discard) ExecuteDiscard(request.game); else Execute(request);
        std::lock_guard<std::mutex> lock(mutex_); busy_ = false;
        if (state_->active) {
            auto next = std::make_shared<JobSnapshot>(*state_); ++next->revision;
            next->active = false; next->cancelling = false; state_ = std::move(next);
        }
    }
}
}  // namespace playden::stores::xbox
