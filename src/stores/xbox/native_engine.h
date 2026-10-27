// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "stores/xbox/jobs.h"
namespace playden::stores::xbox {
// Available only when linked with the optional native Xbox engine.
NativeRunner NativeEngine(std::string root, std::string games, std::string trust, std::function<void(const std::string&)> log = {});
}
