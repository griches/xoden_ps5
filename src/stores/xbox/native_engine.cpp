// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/native_engine.h"
#include "xodus_ps5_native.h"
#include <chrono>
#include <cstring>
#include <thread>
namespace playden::stores::xbox {
namespace {
std::string Message(const std::string& phase) {
    if (phase == "checking-account") return "Checking Xbox access…";
    if (phase == "resolving") return "Finding the PC download…";
    if (phase == "metadata") return "Reading game data…";
    if (phase == "downloading") return "Downloading Xbox game";
    if (phase == "preparing") return "Preparing game files…";
    if (phase == "finalizing") return "Finishing download…";
    return "Starting Xbox download…";
}
}
NativeRunner NativeEngine(std::string root, std::string games, std::string trust) {
    return [root = std::move(root),games = std::move(games),trust = std::move(trust)](
        const std::function<void(const NativeProgress&)>& progress,
        const std::function<bool()>& cancelled) {
        XodusJobConfig config{reinterpret_cast<const uint8_t*>(root.data()),root.size(),
            reinterpret_cast<const uint8_t*>(games.data()),games.size(),
            reinterpret_cast<const uint8_t*>(trust.data()),trust.size()};
        std::uint64_t handle = 0;
        if (xodus_job_start(&config,sizeof(config),XODUS_NATIVE_ABI,&handle) != 0) return NativeResult::Failed;
        bool cancel_sent = false;
        for (;;) {
            if (!cancel_sent && cancelled()) cancel_sent = xodus_job_cancel(handle) == 0;
            XodusJobSnapshot state{};
            if (xodus_job_poll(handle,&state,sizeof(state),XODUS_NATIVE_ABI) == 0) {
                if (state.state >= 4 && state.state <= 6) {
                    // Release succeeds only after the runtime and all file writes
                    // have drained. Never permit cleanup on a polling failure.
                    if (xodus_job_release(handle) == 0)
                        return state.state == 4 ? NativeResult::Complete : state.state == 6 ? NativeResult::Cancelled : NativeResult::Failed;
                }
                const std::string phase(state.phase,strnlen(state.phase,sizeof(state.phase)));
                progress({phase,Message(phase),state.completed_bytes,(state.flags & 1) ? state.total_bytes : 0,state.bytes_per_second});
            } else {
                xodus_job_cancel(handle);
                progress({"stopping","Waiting for the Xbox downloader to stop safely…"});
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(250));
        }
    };
}
}
