// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <cstdint>
#include <string>
namespace playden::runtime {
// Caller holds the stopped-title gate. Copies the separately provisioned Xbox
// runtime into a private bottle and enables its native web implementation.
bool InstallXboxRuntime(const std::string& source, const std::string& prefix,
                        std::int64_t unix_time, std::string* reason);
}
