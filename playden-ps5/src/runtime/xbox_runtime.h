// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <cstdint>
#include <string>
#include <string_view>
namespace playden::runtime {
// Caller holds the stopped-title gate. Copies the separately provisioned Xbox
// runtime into a private bottle and enables its native web implementation.
bool InstallXboxRuntime(const std::string& source, const std::string& prefix,
                        std::int64_t unix_time, std::string* reason, const std::string& wrapper_source = {});
// Copies the pinned, unmodified Microsoft x64 compiler into a stopped game's
// bottle. Does not change the kit or any other game's libraries.
bool InstallD3DCompiler43(const std::string& source, const std::string& prefix, std::string* reason);
// Fixed diagnostic markers only; never publishes raw bridge errors or tokens.
std::string XboxBridgeDiagnostics(std::string_view log);
}
