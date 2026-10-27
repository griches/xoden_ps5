// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/library_source.h"
#include <utility>
#include <algorithm>

namespace playden::stores::xbox {
XboxLibrarySource::XboxLibrarySource(std::string root) : storage_(std::move(root)), worker_([this] { Run(); }) {}
XboxLibrarySource::~XboxLibrarySource() { Shutdown(); }
void XboxLibrarySource::Shutdown() {
    { std::lock_guard<std::mutex> lock(mutex_); stopping_ = true;
      jobs_.erase(std::remove_if(jobs_.begin(), jobs_.end(), [](const Job& job) { return !job.clear; }), jobs_.end()); }
    wake_.notify_all();
    if (worker_.joinable()) worker_.join();
}
std::uint64_t XboxLibrarySource::Submit(std::uint64_t account, std::uint64_t generation, bool clear) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (stopping_ || clear_barrier_ || generation < generation_ || jobs_.size() >= 8) return 0;
    if (generation != generation_ || clear) ++epoch_;
    generation_ = generation;
    if (clear) { clear_barrier_ = true; jobs_.clear(); }
    const auto request = ++next_;
    jobs_.push_back({request, account, generation, epoch_, clear});
    wake_.notify_one();
    return request;
}
std::uint64_t XboxLibrarySource::Refresh(std::uint64_t account, std::uint64_t generation) { return Submit(account, generation, false); }
std::uint64_t XboxLibrarySource::LoadCached(std::uint64_t account, std::uint64_t generation) { return Submit(account, generation, false); }
std::uint64_t XboxLibrarySource::Clear(std::uint64_t account, std::uint64_t generation) { return Submit(account, generation, true); }
void XboxLibrarySource::Invalidate(std::uint64_t generation) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (generation < generation_) return;
    generation_ = generation; ++epoch_;
    jobs_.erase(std::remove_if(jobs_.begin(), jobs_.end(), [](const Job& job) { return !job.clear; }), jobs_.end());
    events_.clear(); snapshot_.reset();
}
std::optional<Catalog> XboxLibrarySource::Snapshot() const {
    std::lock_guard<std::mutex> lock(mutex_); return snapshot_;
}
std::vector<LibraryEvent> XboxLibrarySource::Pump(std::size_t max_events) {
    std::lock_guard<std::mutex> lock(mutex_);
    std::vector<LibraryEvent> result;
    while (!events_.empty() && result.size() < max_events) { result.push_back(std::move(events_.front())); events_.pop_front(); }
    return result;
}
void XboxLibrarySource::Run() {
    for (;;) {
        Job job;
        {
            std::unique_lock<std::mutex> lock(mutex_);
            wake_.wait(lock, [this] { return stopping_ || !jobs_.empty(); });
            if (stopping_ && jobs_.empty()) return;
            job = jobs_.front(); jobs_.pop_front();
            if (!job.clear && job.epoch != epoch_) continue;
        }
        LibraryEvent event;
        event.request_id = job.request; event.account_id = job.account; event.account_generation = job.generation;
        std::optional<Catalog> catalog;
        if (job.clear) {
            // Clearing this source never logs out or removes installed games.
            const auto removed = storage_.Remove("catalog.json");
            event.kind = removed.ok() ? LibraryEventKind::kCleared : LibraryEventKind::kFailure;
            event.error = removed.ok() ? steam::LibraryError::kNone : steam::LibraryError::kPersistence;
        } else {
            const auto bytes = storage_.Read("catalog.json", 16 * 1024 * 1024);
            if (bytes.ok()) catalog = ParseCatalog(std::string_view(reinterpret_cast<const char*>(bytes.value().data()), bytes.value().size()));
            if (catalog) {
                event.kind = LibraryEventKind::kComplete;
                event.games = catalog->games; event.cache_available = true; event.metadata_partial = catalog->partial;
            } else if (!bytes.ok() && bytes.error() == StorageError::kNotFound) {
                event.kind = LibraryEventKind::kComplete; // No Xbox session/cache provisioned yet.
            } else {
                event.kind = LibraryEventKind::kFailure;
                event.error = bytes.ok() ? steam::LibraryError::kProtocol : steam::LibraryError::kPersistence;
            }
        }
        std::lock_guard<std::mutex> lock(mutex_);
        if (job.clear) clear_barrier_ = false;
        else if (stopping_ || job.epoch != epoch_) continue;
        snapshot_ = std::move(catalog);
        if (events_.size() == 16) events_.pop_front();
        events_.push_back(std::move(event));
    }
}
}  // namespace playden::stores::xbox
