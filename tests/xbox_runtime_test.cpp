// SPDX-License-Identifier: GPL-3.0-or-later
#include "runtime/xbox_runtime.h"
#include "platform/storage.h"
#include <cassert>
#include <cstdio>
#include <algorithm>
#include <filesystem>
#include <unistd.h>
int main() {
    char temp[]="/tmp/playden-xbox-runtime-XXXXXX";assert(mkdtemp(temp));const auto root=std::filesystem::canonical(temp);
    std::filesystem::create_directories(root/"runtime");std::filesystem::create_directories(root/"prefix/drive_c/windows/system32");
    std::filesystem::permissions(root/"prefix",std::filesystem::perms::owner_all);
    playden::Storage source((root/"runtime").string()),prefix((root/"prefix").string()),system32((root/"prefix/drive_c/windows/system32").string());
    const std::string registry="WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\DllOverrides] 0\n\"existing\"=\"builtin\"\n";
    assert(prefix.WriteAtomicPrivate("user.reg",{registry.begin(),registry.end()}).ok());
    playden::Bytes dll(256);dll[0]='M';dll[1]='Z';dll[60]=128;dll[128]='P';dll[129]='E';dll[132]=0x64;dll[133]=0x86;
    for(const char* name:{"xgameruntime.dll","xgameruntime_impl.dll","windows.web.dll"}) assert(source.WriteAtomicPrivate(name,dll).ok());
    auto builtin = dll;
    const std::string marker = "Wine builtin DLL";
    std::copy(marker.begin(),marker.end(),builtin.begin()+64);
    assert(source.WriteAtomicPrivate("windows.web.dll",builtin).ok());
    std::string reason;
    const bool installed = playden::runtime::InstallXboxRuntime((root/"runtime").string(),(root/"prefix").string(),123,&reason);
    if (!installed) std::fprintf(stderr,"%s\n",reason.c_str());
    assert(installed);
    const auto first=prefix.Read("user.reg");assert(first.ok());const std::string text(first.value().begin(),first.value().end());
    assert(text.find("\"windows.web\"=\"native\"")!=std::string::npos && text.find("\"existing\"=\"builtin\"")!=std::string::npos);
    assert(playden::runtime::InstallXboxRuntime((root/"runtime").string(),(root/"prefix").string(),456,&reason));
    assert(prefix.Read("user.reg").value()==first.value());
    assert(system32.Read("xgameruntime.dll").value()==dll);
    assert(system32.Read("windows.web.dll").value()==dll);
    assert(source.WriteAtomicPrivate("windows.web.dll",{'b','a','d'}).ok());
    assert(!playden::runtime::InstallXboxRuntime((root/"runtime").string(),(root/"prefix").string(),789,&reason));
    assert(system32.Read("windows.web.dll").value()==dll);
    assert(source.WriteAtomicPrivate("windows.web.dll",dll).ok());
    std::filesystem::remove(root/"prefix/drive_c/windows/system32/windows.web.dll");
    std::filesystem::create_symlink(root/"runtime/windows.web.dll",root/"prefix/drive_c/windows/system32/windows.web.dll");
    assert(!playden::runtime::InstallXboxRuntime((root/"runtime").string(),(root/"prefix").string(),789,&reason));
    std::filesystem::remove_all(root);
}
