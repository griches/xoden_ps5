// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/jobs.h"
#include "steam/backend/json.h"
#include <cassert>
#include <atomic>
#include <chrono>
#include <filesystem>
#include <thread>
#include <unistd.h>
using namespace playden;
namespace {
void Wait(stores::xbox::Jobs& jobs, const char* phase) {
    for (int i=0;i<2000;++i) { if (jobs.Snapshot()->phase == phase) return; std::this_thread::sleep_for(std::chrono::milliseconds(1)); }
    fprintf(stderr,"Expected phase %s, got %s\n",phase,jobs.Snapshot()->phase.c_str());
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
    { stores::xbox::Jobs reload(config);Wait(reload,"idle");assert(reload.Snapshot()->prepared.size()==1);assert(!reload.Snapshot()->completed_game.valid()); }
    Put(store,"prepared-games.json","{\"schema\":1,\"games\":[{\"productId\":\"9N5PKH0KW2L1\",\"accountKey\":\""+std::string(64,'a')+"\",\"folder\":\"xbox-9n5pkh0kw2l1\",\"executable\":\"../escape.exe\"}]}");
    { stores::xbox::Jobs reload(config);Wait(reload,"idle");assert(reload.Snapshot()->prepared.empty()); }
    // Orphan partials remain manageable even after a newer job finishes.
    Put(store,"prepared-games.json","{\"schema\":1,\"games\":[]}");
    const auto partial = root/"games/xbox-9np1p1wfs0lb";
    const auto quarantine = root/"games/.delete-xbox-9np1p1wfs0lb";
    std::filesystem::create_directories(partial/"nested");
    Storage partial_store(partial.string());
    Put(partial_store,".nexus-streaming-v1.jsonl","resume");
    Put(partial_store,"nested/content.bin","partial bytes");
    {
        stores::xbox::Jobs cleanup(config); Wait(cleanup,"idle");
        assert(cleanup.Snapshot()->partials.size() == 1);
        assert(!cleanup.Discard(GameId::Xbox("9N5PKH0KW2L1")));
        assert(cleanup.Discard(GameId::Xbox("9NP1P1WFS0LB"))); Wait(cleanup,"cleaned");
        assert(!std::filesystem::exists(partial));
        assert(cleanup.Snapshot()->partials.empty());
    }
    // A symlink in partial content is refused; its destination is untouched and
    // the resume marker survives to allow a subsequent cleanup attempt.
    std::filesystem::create_directories(partial);
    Put(partial_store,".nexus-streaming-v1.jsonl","resume");
    Put(store,"keep.txt","saved data");
    std::filesystem::create_symlink(root/"keep.txt",partial/"link");
    {
        stores::xbox::Jobs cleanup(config); Wait(cleanup,"idle");
        assert(cleanup.Discard(GameId::Xbox("9NP1P1WFS0LB"))); Wait(cleanup,"failed");
        assert(std::filesystem::exists(root/"keep.txt"));
        assert(std::filesystem::exists(quarantine/".nexus-streaming-v1.jsonl"));
        assert(!std::filesystem::exists(partial));
    }
    // Prepared and corrupt manifests both prevent cleanup admission.
    Put(store,"prepared-games.json","{\"schema\":1,\"games\":[{\"productId\":\"9NP1P1WFS0LB\",\"folder\":\"xbox-9np1p1wfs0lb\"}]}");
    {
        stores::xbox::Jobs cleanup(config); Wait(cleanup,"idle");
        assert(cleanup.Snapshot()->partials.empty());
        assert(!cleanup.Discard(GameId::Xbox("9NP1P1WFS0LB")));
    }
    Put(store,"prepared-games.json","broken");
    {
        stores::xbox::Jobs cleanup(config); Wait(cleanup,"idle");
        assert(cleanup.Snapshot()->partials.empty());
        assert(!cleanup.Discard(GameId::Xbox("9NP1P1WFS0LB")));
    }
    std::filesystem::remove(quarantine/"link");
    // Simulate a crash after the final content/marker deletion. The empty
    // quarantine is sufficient to resume; a new install must not race it.
    std::filesystem::remove(quarantine/".nexus-streaming-v1.jsonl");
    Put(store,"prepared-games.json",R"({"schema":1,"games":[]})");
    {
        stores::xbox::Jobs cleanup(config); Wait(cleanup,"idle");
        assert(cleanup.Snapshot()->partials.size() == 1);
        assert(cleanup.Install(GameId::Xbox("9NP1P1WFS0LB"),std::string(64,'a'),"GB")); Wait(cleanup,"failed");
        cleanup.Shutdown();
    }
    {
        stores::xbox::Jobs cleanup(config); Wait(cleanup,"idle");
        assert(cleanup.Discard(GameId::Xbox("9NP1P1WFS0LB"))); Wait(cleanup,"cleaned");
        assert(!std::filesystem::exists(quarantine));
    }
    std::filesystem::create_directories(partial);
    std::filesystem::remove(root/"prepared-games.json");
    Put(store,"job-request.json",R"({"schema":1,"requestId":"11111111-1111-1111-1111-111111111111","operation":"install","productId":"9NP1P1WFS0LB"})");
    Put(store,"job-result.json",R"({"schema":1,"requestId":"11111111-1111-1111-1111-111111111111","productId":"9NP1P1WFS0LB","status":"downloading"})");
    std::filesystem::remove(partial/".nexus-streaming-v1.jsonl");
    Put(partial_store,".xodus-streaming-tmp.msixvc","metadata");
    {
        stores::xbox::Jobs cleanup(config); Wait(cleanup,"failed"); // interrupted download
        assert(cleanup.Snapshot()->partials.size() == 1);
        assert(cleanup.Discard(GameId::Xbox("9NP1P1WFS0LB"))); Wait(cleanup,"cleaned");
        assert(!std::filesystem::exists(partial));
        const auto bytes = store.Read("job-result.json"); assert(bytes.ok());
        const auto result = steam::json::Parse(std::string(bytes.value().begin(),bytes.value().end()));
        assert(result && result->Find("status")->text() == "cancelled");
    }
    { stores::xbox::Jobs reload(config); Wait(reload,"idle"); assert(reload.Snapshot()->partials.empty()); }
    // Native downloads remain in Playden, expose progress, and cannot delete
    // content until the runner confirms that outstanding writes have drained.
    std::filesystem::remove(root/"worker.json");
    Put(store,"prepared-games.json",R"({"schema":1,"games":[]})");
    std::atomic<bool> saw_cancel{false}, allow_drain{false};
    auto native_config = config;
    native_config.handoff = [&](const auto&, const auto&, auto*) { assert(false); return false; };
    native_config.native_run = [&](const auto& report,const auto& cancelled) {
        std::filesystem::create_directories(partial);
        Put(partial_store,".nexus-streaming-v1.jsonl","resume");
        Put(partial_store,"content.bin","partial content");
        report(stores::xbox::NativeProgress{"downloading","Downloading Xbox game",40,100,9});
        while (!cancelled()) std::this_thread::sleep_for(std::chrono::milliseconds(1));
        saw_cancel = true;
        while (!allow_drain) std::this_thread::sleep_for(std::chrono::milliseconds(1));
        return stores::xbox::NativeResult::Cancelled;
    };
    {
        stores::xbox::Jobs native(native_config); Wait(native,"idle");
        const auto game = GameId::Xbox("9NP1P1WFS0LB");
        assert(native.Snapshot()->native && native.Snapshot()->worker_title.empty());
        assert(native.Install(game,std::string(64,'a'),"GB")); Wait(native,"downloading");
        assert(native.Snapshot()->active && native.Snapshot()->active_game == game);
        assert(native.Snapshot()->done_bytes == 40 && native.Snapshot()->total_bytes == 100);
        assert(!native.Discard(game));
        assert(!native.Install(GameId::Xbox("9N5PKH0KW2L1"),std::string(64,'a'),"GB"));
        assert(native.Cancel(game));
        for (int i=0;i<2000 && !saw_cancel;++i) std::this_thread::sleep_for(std::chrono::milliseconds(1));
        assert(saw_cancel && native.Snapshot()->cancelling);
        assert(std::filesystem::exists(partial/"content.bin"));
        assert(!std::filesystem::exists(quarantine));
        allow_drain = true; Wait(native,"cleaned");
        assert(!std::filesystem::exists(partial) && !std::filesystem::exists(quarantine));
        assert(native.Snapshot()->partials.empty());
    }
    // Pause drains IO just like cancel, but retains content and offers recovery.
    saw_cancel = false; allow_drain = false;
    {
        stores::xbox::Jobs native(native_config); Wait(native,"idle");
        const auto game = GameId::Xbox("9NP1P1WFS0LB");
        assert(!native.Install(game,std::string(64,'a'),"GB","bad-content"));
        assert(native.Install(game,std::string(64,'a'),"GB","11111111-1111-4111-8111-111111111111"));
        Wait(native,"downloading"); assert(native.Pause(game));
        assert(!native.Cancel(game));
        for (int i=0;i<2000 && !saw_cancel;++i) std::this_thread::sleep_for(std::chrono::milliseconds(1));
        assert(saw_cancel && std::filesystem::exists(partial/"content.bin"));
        const auto pending_request = store.Read("job-request.json"); assert(pending_request.ok());
        const auto pending_value = steam::json::Parse(std::string(pending_request.value().begin(),pending_request.value().end()));
        Put(store,"job-result.json","{\"schema\":1,\"requestId\":\"" + pending_value->Find("requestId")->text() + "\",\"productId\":\"9NP1P1WFS0LB\",\"status\":\"cancelled\"}");
        allow_drain = true; Wait(native,"cancelled");
        assert(std::filesystem::exists(partial/"content.bin"));
        assert(native.Snapshot()->partials.size() == 1);
        const auto request = store.Read("job-request.json"); assert(request.ok());
        const auto value = steam::json::Parse(std::string(request.value().begin(),request.value().end()));
        assert(value->Find("contentId")->text() == "11111111-1111-4111-8111-111111111111");
    }
    {
        stores::xbox::Jobs recovered(native_config); Wait(recovered,"cancelled");
        // A reopened app restores the paused byte counters without network IO.
        assert(recovered.Snapshot()->done_bytes == 40 && recovered.Snapshot()->total_bytes == 100);
    }
    // Another operation cannot erase the selected edition used by Resume.
    Put(store,"job-request.json",R"({"schema":1,"requestId":"refresh","operation":"refresh","market":"GB"})");
    native_config.native_run = [&](const auto&,const auto&) {
        const auto bytes = store.Read("job-request.json"); assert(bytes.ok());
        const auto request = steam::json::Parse(std::string(bytes.value().begin(),bytes.value().end()));
        assert(request->Find("contentId")->text() == "11111111-1111-4111-8111-111111111111");
        return stores::xbox::NativeResult::Failed;
    };
    {
        stores::xbox::Jobs native(native_config); Wait(native,"idle");
        assert(native.Install(GameId::Xbox("9NP1P1WFS0LB"),std::string(64,'a'),"GB")); Wait(native,"failed");
    }
    std::filesystem::remove_all(partial);
    // Fresh completion requests preparation, while startup only inventories files.
    native_config.native_run = [&](const auto&, const auto&) {
        std::filesystem::create_directories(root/"games"/"xbox-9n5pkh0kw2l1");
        Put(store,"prepared-games.json","{\"schema\":1,\"games\":[{\"productId\":\"9N5PKH0KW2L1\",\"accountKey\":\""+std::string(64,'a')+"\",\"folder\":\"xbox-9n5pkh0kw2l1\",\"executable\":\"Game.exe\"}]}");
        return stores::xbox::NativeResult::Complete;
    };
    {
        stores::xbox::Jobs native(native_config); Wait(native,"idle");
        assert(native.Install(GameId::Xbox("9N5PKH0KW2L1"),std::string(64,'a'),"GB"));
        for (int i=0;i<2000 && !native.Snapshot()->completed_game.valid();++i)
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
        assert(native.Snapshot()->completed_game == GameId::Xbox("9N5PKH0KW2L1"));
        assert(!native.Snapshot()->request_id.empty());
    }
    { stores::xbox::Jobs reload(native_config); Wait(reload,"idle"); assert(!reload.Snapshot()->completed_game.valid()); }
    // Installed content is admitted only from its prepared manifest, then
    // removed without touching another game's content or any saves.
    const auto installed = root/"games/xbox-9n5pkh0kw2l1";
    const auto other = root/"games/xbox-9mxnd4pqlk3w";
    std::filesystem::create_directories(installed); std::filesystem::create_directories(other);
    Storage installed_store(installed.string()), other_store(other.string());
    Put(installed_store,"Game.exe","content"); Put(other_store,"Other.exe","keep");
    Put(store,"prepared-games.json","{\"schema\":1,\"games\":[{\"productId\":\"9N5PKH0KW2L1\",\"accountKey\":\""+std::string(64,'a')+"\",\"folder\":\"xbox-9n5pkh0kw2l1\",\"executable\":\"Game.exe\"},{\"productId\":\"9MXND4PQLK3W\",\"accountKey\":\""+std::string(64,'a')+"\",\"folder\":\"xbox-9mxnd4pqlk3w\",\"executable\":\"Other.exe\"}]}");
    {
        stores::xbox::Jobs uninstall(config); Wait(uninstall,"idle");
        assert(!uninstall.Uninstall(GameId::Xbox("9NP1P1WFS0LB")));
        assert(uninstall.Uninstall(GameId::Xbox("9N5PKH0KW2L1"))); Wait(uninstall,"cleaned");
        assert(!std::filesystem::exists(installed));
        assert(std::filesystem::exists(other/"Other.exe"));
        assert(uninstall.Snapshot()->prepared.size() == 1 && uninstall.Snapshot()->prepared.front().game == GameId::Xbox("9MXND4PQLK3W"));
    }
    { stores::xbox::Jobs reload(config); Wait(reload,"idle"); assert(reload.Snapshot()->prepared.size() == 1); }
    // Refresh uses the native service without download identity or cleanup.
    native_config.native_run = [&](const auto& report,const auto&) {
        const auto bytes = store.Read("job-request.json"); assert(bytes.ok());
        const auto request = steam::json::Parse(std::string(bytes.value().begin(),bytes.value().end()));
        assert(request && request->Find("operation")->text() == "refresh");
        assert(!request->Find("productId") && !request->Find("accountKey"));
        report(stores::xbox::NativeProgress{"refreshing","Refreshing Xbox library"});
        Put(store,"job-result.json","{\"schema\":1,\"requestId\":\"" + request->Find("requestId")->text() + "\",\"operation\":\"refresh\",\"status\":\"complete\"}");
        return stores::xbox::NativeResult::Complete;
    };
    {
        stores::xbox::Jobs account(native_config); Wait(account,"idle");
        assert(!account.Refresh("invalid")); assert(account.Refresh("GB")); Wait(account,"refreshed");
        assert(!account.Snapshot()->request_id.empty()); assert(!account.Snapshot()->completed_game.valid());
    }
    // New native installs resolve a plan without creating download content.
    // Only an explicit package selection authorizes the actual install request.
    native_config.native_run = [&](const auto&,const auto&) {
        const auto bytes = store.Read("job-request.json"); assert(bytes.ok());
        const auto request = steam::json::Parse(std::string(bytes.value().begin(),bytes.value().end()));
        assert(request);
        assert(request->Find("operation")->text() == (request->Find("contentId") ? "install" : "plan"));
        assert(!std::filesystem::exists(root/"games/xbox-9n5pkh0kw2l1"));
        return stores::xbox::NativeResult::Failed;
    };
    {
        stores::xbox::Jobs planning(native_config); Wait(planning,"refreshed");
        assert(planning.Install(GameId::Xbox("9N5PKH0KW2L1"),std::string(64,'a'),"GB")); Wait(planning,"failed");
        assert(planning.Install(GameId::Xbox("9N5PKH0KW2L1"),std::string(64,'a'),"GB","11111111-1111-4111-8111-111111111111")); Wait(planning,"failed");
    }
    // Only bounded, matching, supported PC package choices reach the UI.
    Put(store,"job-request.json",R"({"schema":1,"requestId":"choice","operation":"install","market":"GB","productId":"9N5PKH0KW2L1"})");
    Put(store,"job-result.json",R"({"schema":1,"requestId":"choice","status":"package-choice","detail":{"rootProductId":"9N5PKH0KW2L1","market":"GB","choices":[{"contentId":"11111111-1111-4111-8111-111111111111","title":"PC edition","platform":"Windows.Desktop","packageFormat":"MSIXVC","supported":true,"downloadBytes":1234},{"contentId":"22222222-2222-4222-8222-222222222222","title":"Console edition","platform":"XboxOne","packageFormat":"MSIXVC","supported":true}]}})");
    {
        stores::xbox::Jobs choices(config); Wait(choices,"package-choice");
        const auto snapshot = choices.Snapshot();
        assert(snapshot->choice_game == GameId::Xbox("9N5PKH0KW2L1"));
        assert(snapshot->choices.size() == 1 && snapshot->choices[0].download_bytes == 1234);
    }
    std::filesystem::remove_all(root);
}
