// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "library/model.h"
#include <optional>
#include <string_view>
#include <vector>

namespace playden::stores::xbox {
struct Catalog {
    std::string account_key;
    std::string gamertag;
    std::string market;
    bool subscription_active = false;
    bool partial = false;
    std::vector<GameSummary> games;
};
// The worker publishes a complete snapshot. The account fingerprint is opaque;
// callers bind it to their selected account before presenting or installing.
std::optional<Catalog> ParseCatalog(std::string_view bytes);
}  // namespace playden::stores::xbox
