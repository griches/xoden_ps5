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
struct JobSnapshot {
    std::uint64_t revision = 0;
    std::string phase = "loading", message, request_id;
    std::string worker_install_id, worker_title;
    std::vector<PreparedGame> prepared;
};
struct JobConfig {
    std::string root, games_root;
    // Runs on this service's worker, after the private request is committed.
    // Success means the console handoff helper acknowledged this request ID.
    std::function<bool(const std::string& request, const std::string& worker_title, std::string* error)> handoff;
    std::function<std::shared_ptr<void>()> hold_effective_uid;
};
class Jobs {
public:
    explicit Jobs(JobConfig config);
    ~Jobs();
    bool Install(GameId game, std::string account_key, std::string market);
    std::shared_ptr<const JobSnapshot> Snapshot() const;
    void Shutdown();
private:
    struct Request { GameId game; std::string account, market; };
    void Run();
    void Load();
    void Execute(const Request& request);
    void Publish(std::string phase, std::string message = {});
    JobConfig config_;
    Storage storage_;
    mutable std::mutex mutex_;
    std::condition_variable wake_;
    std::optional<Request> pending_;
    std::shared_ptr<JobSnapshot> state_;
    bool stopping_ = false, busy_ = false;
    std::thread worker_;
};
}  // namespace playden::stores::xbox
