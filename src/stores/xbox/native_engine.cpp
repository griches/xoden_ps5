// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/native_engine.h"
#include "steam/install_volumes.h"
#include "xodus_ps5_native.h"
#include <chrono>
#include <cstring>
#include <thread>
#include <cerrno>
#include <fcntl.h>
#include <unistd.h>
#include <sys/event.h>
#include <sys/socket.h>
extern "C" int getentropy(void *, size_t);
extern "C" int xodus_ps5_entropy_status(void);
extern "C" int xodus_ps5_available_space(const char* path, std::uint64_t* bytes) {
    if (!path || !bytes || path[0] != '/') return -1;
    try {
        const auto space = playden::steam::QueryVolumeSpace(path);
        if (!space) return -1;
        *bytes = space->free_bytes;
        return 0;
    } catch (...) {
        return -1; // Never unwind across the Rust/C boundary.
    }
}
namespace playden::stores::xbox {
namespace {
std::string Message(const std::string& phase) {
    if (phase == "refreshing") return "Refreshing Xbox library…";
    if (phase == "checking-account") return "Checking Xbox access…";
    if (phase == "resolving") return "Finding the PC download…";
    if (phase == "metadata") return "Reading game data…";
    if (phase == "downloading") return "Downloading Xbox game";
    if (phase == "preparing") return "Preparing game files…";
    if (phase == "finalizing") return "Finishing download…";
    return "Starting Xbox download…";
}
}
NativeRunner NativeEngine(std::string root, std::string games, std::string trust, std::function<void(const std::string&)> log) {
    return [root = std::move(root),games = std::move(games),trust = std::move(trust), log = std::move(log)](
        const std::function<void(const NativeProgress&)>& progress,
        const std::function<bool()>& cancelled) {
        // Load and verify the console entropy provider before Rust's infallible
        // hash-seed API is reached. Missing modules become a failed job.
        unsigned char entropy_probe;
        if (getentropy(&entropy_probe,1) != 0) {
            if (log) log("native entropy unavailable code=" + std::to_string(xodus_ps5_entropy_status()));
            return NativeResult::Failed;
        }
        if (log) log("native entropy ready");
        if (log) {
            const int socket_flags = socket(AF_INET,SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK,0);
            log("native io socket-flags=" + std::to_string(socket_flags) + " errno=" + std::to_string(socket_flags < 0 ? errno : 0));
            if (socket_flags >= 0) close(socket_flags);
            const int queue = kqueue();
            log("native io queue=" + std::to_string(queue) + " errno=" + std::to_string(queue < 0 ? errno : 0));
            if (queue >= 0) {
                const int flags = fcntl(queue,F_SETFD,FD_CLOEXEC);
                log("native io cloexec=" + std::to_string(flags) + " errno=" + std::to_string(flags < 0 ? errno : 0));
                const int duplicate = fcntl(queue,F_DUPFD_CLOEXEC,3);
                log("native io dup-cloexec=" + std::to_string(duplicate) + " errno=" + std::to_string(duplicate < 0 ? errno : 0));
                if (duplicate >= 0) close(duplicate);
                const int plain_duplicate = fcntl(queue,F_DUPFD,3);
                log("native io dup=" + std::to_string(plain_duplicate) + " errno=" + std::to_string(plain_duplicate < 0 ? errno : 0));
                if (plain_duplicate >= 0) close(plain_duplicate);
                struct kevent change{}, event{}; struct timespec timeout{};
                EV_SET(&change,0,EVFILT_USER,EV_ADD | EV_CLEAR | EV_RECEIPT,0,0,nullptr);
                const int receipt = kevent(queue,&change,1,&event,1,&timeout);
                log("native io receipt=" + std::to_string(receipt) + " errno=" + std::to_string(receipt < 0 ? errno : 0) +
                    " flags=" + std::to_string(event.flags) + " data=" + std::to_string(event.data));
                close(queue);
            }
        }
        XodusJobConfig config{reinterpret_cast<const uint8_t*>(root.data()),root.size(),
            reinterpret_cast<const uint8_t*>(games.data()),games.size(),
            reinterpret_cast<const uint8_t*>(trust.data()),trust.size()};
        std::uint64_t handle = 0;
        const int started = xodus_job_start(&config,sizeof(config),XODUS_NATIVE_ABI,&handle);
        if (log) log("native job start code=" + std::to_string(started));
        if (started != 0) return NativeResult::Failed;
        bool cancel_sent = false;
        for (;;) {
            if (!cancel_sent && cancelled()) cancel_sent = xodus_job_cancel(handle) == 0;
            XodusJobSnapshot state{};
            if (xodus_job_poll(handle,&state,sizeof(state),XODUS_NATIVE_ABI) == 0) {
                if (state.state >= 4 && state.state <= 6) {
                    // Release succeeds only after the runtime and all file writes
                    // have drained. Never permit cleanup on a polling failure.
                    if (xodus_job_release(handle) == 0) {
                        if (log) log("native job terminal state=" + std::to_string(state.state) + " error=" +
                            std::string(state.error,strnlen(state.error,sizeof(state.error))));
                        return state.state == 4 ? NativeResult::Complete : state.state == 6 ? NativeResult::Cancelled : NativeResult::Failed;
                    }
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
