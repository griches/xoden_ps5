// SPDX-License-Identifier: GPL-3.0-or-later
#include "runtime/xbox_runtime.h"
#include "runtime/game_settings.h"
#include "platform/managed_directory.h"
#include <unistd.h>
#include <cerrno>
#include <algorithm>
#include <cstring>
#include <optional>

namespace playden::runtime {
std::string XboxBridgeDiagnostics(std::string_view log) {
    std::string result;
    while (!log.empty()) {
        const auto end=log.find('\n');const auto line=log.substr(0,end);
        const char* marker=nullptr;
        if (line.find(" start pid ")!=line.npos) marker="started";
        else if (line.find(" listening")!=line.npos) marker="listening";
        else if (line.find(" MSA token for ")!=line.npos) marker=line.find("full trust true")!=line.npos?"msa-request-full-trust":"msa-request-signin";
        else if (line.find("Phone test MSA service ticket issued")!=line.npos) marker="phone-ticket-issued";
        else if (line.find("MSA token issued")!=line.npos) marker="legacy-ticket-issued";
        else if (line.find("Microsoft rejected the phone runtime scope")!=line.npos) marker="phone-scope-rejected";
        else if (line.find("Phone runtime refresh network failure")!=line.npos) marker="phone-refresh-network";
        else if (line.find("Phone runtime refresh invalid response")!=line.npos) marker="phone-refresh-response";
        else if (line.find("Phone runtime CA bundle")!=line.npos) marker="phone-trust-unavailable";
        else if (line.find("Microsoft rejected the phone runtime ticket")!=line.npos) marker="phone-ticket-rejected";
        else if (line.find("Microsoft rejected the runtime device ticket")!=line.npos) marker="device-ticket-rejected";
        else if (line.find("Active phone session is invalid")!=line.npos) marker="invalid-phone-session";
        else if (line.find("token store:")!=line.npos) marker="credential-store-unavailable";
        if (marker) { if(result.size()>4096) result.erase(0,result.find('\n')+1);result+="xodus-bridge ";result+=marker;result+='\n'; }
        if (end==log.npos) break;log.remove_prefix(end+1);
    }
    return result;
}
namespace {
constexpr std::size_t kMaximumDll = 32 * 1024 * 1024;
struct File {
    int fd;
    ~File() { if (fd >= 0) close(fd); }
};
bool ReadAt(int fd, unsigned char* bytes, std::size_t size, off_t offset) {
    if (lseek(fd, offset, SEEK_SET) != offset) return false;
    while (size) {
        const auto count = read(fd, bytes, size);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) return false;
        bytes += count; size -= static_cast<std::size_t>(count);
    }
    return true;
}
bool ValidImage(int fd, off_t* size, bool* builtin) {
    struct stat info{};
    unsigned char header[80]{};
    if (fstat(fd, &info) != 0 || !S_ISREG(info.st_mode) || info.st_size < 80 ||
        info.st_size > static_cast<off_t>(kMaximumDll) || !ReadAt(fd, header, sizeof(header), 0)) return false;
    if (header[0] != 'M' || header[1] != 'Z') return false;
    const std::uint32_t offset = header[60] | (std::uint32_t(header[61]) << 8) |
        (std::uint32_t(header[62]) << 16) | (std::uint32_t(header[63]) << 24);
    unsigned char pe[6]{};
    if (offset < 64 || offset > info.st_size - 6 || !ReadAt(fd, pe, sizeof(pe), offset) ||
        std::memcmp(pe, "PE\0\0\x64\x86", 6) != 0) return false;
    *size = info.st_size;
    *builtin = std::memcmp(header + 64, "Wine builtin DLL", 16) == 0;
    return true;
}
bool Transfer(int source, int destination, off_t size, bool patch, bool verify) {
    // Fixed buffers avoid holding multiple complete runtime DLLs in the title heap.
    unsigned char bytes[16 * 1024], existing[16 * 1024];
    if (verify) {
        struct stat info{};
        if (fstat(destination, &info) != 0 || info.st_size != size) return false;
    }
    for (off_t offset = 0; offset < size;) {
        const auto count = static_cast<std::size_t>(std::min<off_t>(sizeof(bytes), size - offset));
        if (!ReadAt(source, bytes, count, offset)) return false;
        if (patch && offset == 0) std::fill(bytes + 64, bytes + 80, 0);
        if (verify) {
            if (!ReadAt(destination, existing, count, offset) || std::memcmp(bytes, existing, count) != 0) return false;
        } else {
            std::size_t written = 0;
            while (written < count) {
                const auto n = write(destination, bytes + written, count - written);
                if (n < 0 && errno == EINTR) continue;
                if (n <= 0) return false;
                written += static_cast<std::size_t>(n);
            }
        }
        offset += static_cast<off_t>(count);
    }
    return true;
}
bool InstallFile(const platform::ManagedDirectory& input, const platform::ManagedDirectory& output,
                 const char* name) {
    File source{input.OpenRegularFile(name)};
    off_t size = 0; bool builtin = false;
    if (source.fd < 0 || !ValidImage(source.fd, &size, &builtin)) return false;
    const bool patch = builtin && std::strcmp(name, "windows.web.dll") == 0;
    struct stat info{};
    if (output.InspectChild(name, &info)) {
        if (!S_ISREG(info.st_mode)) return false;
        File existing{output.OpenRegularFile(name)};
        if (existing.fd < 0) return false;
        if (Transfer(source.fd, existing.fd, size, patch, true)) return true;
    } else if (errno != ENOENT) return false;
    const std::string temporary = std::string(name) + ".xbox-runtime.tmp";
    if (output.InspectChild(temporary, &info)) {
        if (!S_ISREG(info.st_mode) || !output.RemoveRegularFile(temporary)) return false;
    } else if (errno != ENOENT) return false;
    bool ok = false;
    {
        File target{output.CreateExclusiveFile(temporary, 0600)};
        ok = target.fd >= 0 && Transfer(source.fd, target.fd, size, patch, false) && fsync(target.fd) == 0;
    }
    if (ok) {
        File target{output.OpenRegularFile(temporary, true, 0600)};
        ok = target.fd >= 0 && Transfer(source.fd, target.fd, size, patch, true);
    }
    if (ok) ok = output.RenameChild(temporary, output, name) && output.Sync();
    if (!ok) output.RemoveRegularFile(temporary);
    return ok;
}
}
bool InstallXboxRuntime(const std::string& source, const std::string& prefix,
                        std::int64_t unix_time, std::string* reason, const std::string& wrapper_source) {
    using platform::ManagedDirectory; using platform::DirectoryPolicy;
    auto input = ManagedDirectory::Open(source,DirectoryPolicy::kApprovedTree);
    auto output = ManagedDirectory::Open(prefix + "/drive_c/windows/system32",DirectoryPolicy::kApprovedTree);
    auto wrapper = wrapper_source.empty() ? std::optional<ManagedDirectory>{} : ManagedDirectory::Open(wrapper_source,DirectoryPolicy::kApprovedTree);
    if (!wrapper_source.empty() && !wrapper) { *reason="The app's Xbox sign-in bridge is unavailable.";return false; }
    if (!input || !output) { *reason = "The Xbox runtime or the game's Windows folder is unavailable."; return false; }
    for (const char* name : {"xgameruntime.dll","xgameruntime_impl.dll","windows.web.dll"}) {
        const auto& selected=wrapper && std::strcmp(name,"xgameruntime.dll")==0 ? *wrapper : *input;
        if (!InstallFile(selected, *output, name)) {
            *reason = std::string("The Xbox runtime file could not be validated or installed: ") + name;
            return false;
        }
    }
    return ApplyBottleRegistryValues({{"Software\\Wine\\DllOverrides","windows.web","\"native\""}},prefix,unix_time,"user.reg",reason);
}
}
