// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "library/store.h"
#include "platform/storage.h"
#include <condition_variable>
#include <functional>
#include <memory>
#include <mutex>
#include <thread>

namespace playden::stores::xbox {
struct PreparedGame {
    GameId game;
    std::string account_key, executable, root;
};
struct NativeProgress {
    std::string phase, message;
    std::uint64_t done_bytes = 0, total_bytes = 0, bytes_per_second = 0;
};
enum class NativeResult { Complete, Failed, Cancelled };
using NativeRunner = std::function<NativeResult(
    const std::function<void(const NativeProgress&)>&,
    const std::function<bool()>&)>;
struct JobSnapshot {
    std::uint64_t revision = 0;
    std::string phase = "loading", message, request_id;
    std::string worker_install_id, worker_title;
    bool native = false, active = false, cancelling = false;
    // Set only by a successful download in this process, never by startup reload.
    GameId completed_game;
    GameId active_game, removing_game;
    std::string removal_error;
    std::uint64_t done_bytes = 0, total_bytes = 0, bytes_per_second = 0;
    std::vector<PreparedGame> prepared;
    std::vector<GameId> partials;
};
struct JobConfig {
    std::string root, games_root;
    // Synchronous service-worker call. Returns only after all IO has drained.
    NativeRunner native_run;
    // Runs on this service's worker, after the private request is committed.
    // Success means the console handoff helper acknowledged this request ID.
    std::function<bool(const std::string& request, const std::string& worker_title, std::string* error)> handoff;
    std::function<std::shared_ptr<void>()> hold_effective_uid;
};
class Jobs {
public:
    explicit Jobs(JobConfig config);
    ~Jobs();
    bool Discard(GameId game);
    bool Cancel(GameId game);
    // Caller must first finish removing the stopped PS5 runtime title.
    bool Uninstall(GameId game);
    bool Install(GameId game, std::string account_key, std::string market);
    std::shared_ptr<const JobSnapshot> Snapshot() const;
    void Shutdown();
private:
    struct Request { GameId game; std::string account, market; bool discard = false, uninstall = false; };
    void Run();
    void Load();
    void Execute(const Request& request);
    void ExecuteDiscard(GameId game);
    void ExecuteNative(GameId game, const std::string& request_id);
    void ExecuteUninstall(GameId game);
    void Publish(std::string phase, std::string message = {});
    JobConfig config_;
    Storage storage_;
    mutable std::mutex mutex_;
    std::condition_variable wake_;
    std::optional<Request> pending_;
    std::shared_ptr<JobSnapshot> state_;
    bool stopping_ = false, busy_ = false, cancel_requested_ = false;
    std::thread worker_;
};
}  // namespace playden::stores::xbox
