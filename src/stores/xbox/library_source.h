// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "stores/library_source.h"
#include "stores/xbox/catalog.h"
#include <condition_variable>
#include <deque>
#include <mutex>
#include <thread>

namespace playden::stores::xbox {
// Reads the console worker's complete catalog off the UI thread. Refresh here
// reloads the published snapshot; remote refresh is a separate worker job.
class XboxLibrarySource final : public LibrarySource {
public:
    explicit XboxLibrarySource(std::string root);
    ~XboxLibrarySource() override;
    Store store() const override { return Store::kXbox; }
    std::uint64_t Refresh(std::uint64_t account, std::uint64_t generation) override;
    std::uint64_t LoadCached(std::uint64_t account, std::uint64_t generation) override;
    std::uint64_t Clear(std::uint64_t account, std::uint64_t generation) override;
    void Invalidate(std::uint64_t generation) override;
    void Shutdown() override;
    std::vector<LibraryEvent> Pump(std::size_t max_events) override;
    std::optional<Catalog> Snapshot() const;
    std::string AccountKey() const;
private:
    struct Job { std::uint64_t request, account, generation, epoch; bool clear; };
    std::uint64_t Submit(std::uint64_t account, std::uint64_t generation, bool clear);
    void Run();
    Storage storage_;
    mutable std::mutex mutex_;
    std::condition_variable wake_;
    std::deque<Job> jobs_;
    std::deque<LibraryEvent> events_;
    std::optional<Catalog> snapshot_;
    std::uint64_t generation_ = 0, next_ = 0, epoch_ = 0;
    bool stopping_ = false, clear_barrier_ = false;
    std::thread worker_;
};
}  // namespace playden::stores::xbox
