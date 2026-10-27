// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/jobs.h"
#include "steam/backend/json.h"
#include <cassert>
#include <chrono>
#include <filesystem>
#include <thread>
#include <unistd.h>
using namespace playden;
namespace {
void Wait(stores::xbox::Jobs& jobs, const char* phase) {
    for (int i=0;i<2000;++i) { if (jobs.Snapshot()->phase == phase) return; std::this_thread::sleep_for(std::chrono::milliseconds(1)); }
    assert(false);
}
void Put(Storage& store,const char* name,const std::string& text) { assert(store.WriteAtomicPrivate(name,Bytes(text.begin(),text.end())).ok()); }
}
int main() {
    char temp[]="/tmp/playden-xbox-jobs-XXXXXX"; assert(mkdtemp(temp)); const auto root=std::filesystem::canonical(temp);
    Storage store(root.string());
    Put(store,"worker.json",R"({"schema":1,"titleId":"FAKE78001","installId":"local-da50300a-92a4-4c0a-9c60-a2b31cc9f0e3"})");
    bool called=false;
    stores::xbox::JobConfig config; config.root=root.string();config.games_root=(root/"games").string();
    config.handoff=[&](const std::string& id,const std::string& title,std::string*) {
        const auto bytes=store.Read("job-request.json");assert(bytes.ok());
        const auto value=steam::json::Parse(std::string(bytes.value().begin(),bytes.value().end()));assert(value);
        assert(value->Find("requestId")->text()==id && title=="FAKE78001");
        assert(value->Find("accountKey")->text()==std::string(64,'a'));
        assert(value->Find("productId")->text()=="9N5PKH0KW2L1"); called=true;return true;
    };
    stores::xbox::Jobs jobs(config);Wait(jobs,"idle");
    assert(!jobs.Install(GameId::Steam(42),std::string(64,'a'),"GB"));
    assert(!jobs.Install(GameId::Xbox("9N5PKH0KW2L1"),"wrong-account","GB"));
    assert(jobs.Install(GameId::Xbox("9N5PKH0KW2L1"),std::string(64,'a'),"GB"));Wait(jobs,"handoff");assert(called);
    assert(!jobs.Install(GameId::Xbox("9N5PKH0KW2L1"),std::string(64,'a'),"GB"));jobs.Shutdown();
    std::filesystem::create_directories(root/"games/xbox-9n5pkh0kw2l1");
    Put(store,"prepared-games.json","{\"schema\":1,\"games\":[{\"productId\":\"9N5PKH0KW2L1\",\"accountKey\":\""+std::string(64,'a')+"\",\"folder\":\"xbox-9n5pkh0kw2l1\",\"executable\":\"Game.exe\"}]}");
    { stores::xbox::Jobs reload(config);Wait(reload,"idle");assert(reload.Snapshot()->prepared.size()==1); }
    Put(store,"prepared-games.json","{\"schema\":1,\"games\":[{\"productId\":\"9N5PKH0KW2L1\",\"accountKey\":\""+std::string(64,'a')+"\",\"folder\":\"xbox-9n5pkh0kw2l1\",\"executable\":\"../escape.exe\"}]}");
    { stores::xbox::Jobs reload(config);Wait(reload,"idle");assert(reload.Snapshot()->prepared.empty()); }
    std::filesystem::remove_all(root);
}
