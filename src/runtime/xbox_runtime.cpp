// SPDX-License-Identifier: GPL-3.0-or-later
#include "runtime/xbox_runtime.h"
#include "runtime/game_settings.h"
#include "platform/managed_directory.h"
#include "platform/storage.h"
#include <cerrno>

namespace playden::runtime {
namespace {
bool X64(const Bytes& bytes) {
    if (bytes.size() < 64 || bytes[0] != 'M' || bytes[1] != 'Z') return false;
    const std::uint32_t offset = bytes[60] | (std::uint32_t(bytes[61]) << 8) |
        (std::uint32_t(bytes[62]) << 16) | (std::uint32_t(bytes[63]) << 24);
    return offset >= 64 && offset <= bytes.size()-6 && bytes[offset] == 'P' && bytes[offset+1] == 'E' &&
        bytes[offset+2] == 0 && bytes[offset+3] == 0 && bytes[offset+4] == 0x64 && bytes[offset+5] == 0x86;
}
}
bool InstallXboxRuntime(const std::string& source, const std::string& prefix,
                        std::int64_t unix_time, std::string* reason) {
    using platform::ManagedDirectory; using platform::DirectoryPolicy;
    auto input = ManagedDirectory::Open(source,DirectoryPolicy::kApprovedTree);
    auto output = ManagedDirectory::Open(prefix + "/drive_c/windows/system32",DirectoryPolicy::kApprovedTree);
    if (!input || !output) { *reason = "The Xbox runtime or the game's Windows folder is unavailable."; return false; }
    Storage source_files(source), destination(output->path().string());
    for (const char* name : {"xgameruntime.dll","xgameruntime_impl.dll","windows.web.dll"}) {
        struct stat info{};
        if (!input->InspectChild(name,&info) || !S_ISREG(info.st_mode)) { *reason = std::string("The Xbox runtime is missing ") + name; return false; }
        const auto bytes = source_files.Read(name,32*1024*1024);
        if (!bytes.ok() || !X64(bytes.value())) { *reason = std::string("The Xbox runtime file is not a valid x64 image: ") + name; return false; }
        if (output->InspectChild(name,&info)) {
            if (!S_ISREG(info.st_mode)) { *reason = "An Xbox runtime destination is not a regular file."; return false; }
        } else if (errno != ENOENT) { *reason = "An Xbox runtime destination cannot be safely opened."; return false; }
        const auto existing = destination.Read(name,32*1024*1024);
        if (existing.ok() && existing.value() == bytes.value()) continue;
        if (!destination.WriteAtomicPrivate(name,bytes.value()).ok()) { *reason = std::string("The Xbox runtime file could not be installed: ") + name; return false; }
        const auto verified = destination.Read(name,32*1024*1024);
        if (!verified.ok() || verified.value() != bytes.value()) { *reason = "The installed Xbox runtime did not verify."; return false; }
    }
    return ApplyBottleRegistryValues({{"Software\\Wine\\DllOverrides","windows.web","\"native\""}},prefix,unix_time,"user.reg",reason);
}
}
