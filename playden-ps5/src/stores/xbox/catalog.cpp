// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/catalog.h"
#include "steam/backend/json.h"
#include <algorithm>
#include <set>

namespace playden::stores::xbox {
namespace {
using steam::json::Value;
const std::string* Text(const Value& value, const char* name, std::size_t max) {
    const auto* field = value.Find(name);
    if (!field || !field->is_string() || field->text().empty() || field->text().size() > max) return nullptr;
    if (std::any_of(field->text().begin(), field->text().end(), [](unsigned char c) { return c < 32 || c == 127; })) return nullptr;
    return &field->text();
}
bool OptionalText(const Value& value, const char* name, std::size_t max, std::string& result) {
    const auto* field = value.Find(name);
    if (!field || field->is_null()) return true;
    const auto* text = Text(value, name, max);
    if (!text) return false;
    result = *text;
    return true;
}
}  // namespace

std::optional<std::string> ParseHeroArtwork(std::string_view bytes, const GameId& game) {
    if (!game.valid() || game.store != Store::kXbox) return {};
    const auto root=steam::json::Parse(bytes,{2*1024*1024,24,50000,65536});
    const auto* products=root?root->Find("Products"):nullptr;
    if (!products || !products->is_array() || products->array().size()!=1) return {};
    const auto& product=products->array().front();
    const auto* id=Text(product,"ProductId",12);
    const auto* localized=product.Find("LocalizedProperties");
    if (!id || *id!=game.value || !localized || !localized->is_array() || localized->array().empty()) return {};
    const auto* images=localized->array().front().Find("Images");
    if (!images || !images->is_array() || images->array().size()>200) return {};
    for (const char* purpose:{"SuperHeroArt","TitledHeroArt"})
        for (const auto& image:images->array()) {
            const auto* kind=Text(image,"ImagePurpose",32);const auto* uri=Text(image,"Uri",4096);
            if (!kind || *kind!=purpose || !uri) continue;
            const auto url=uri->compare(0,2,"//")==0?"https:"+*uri:*uri;
            constexpr std::string_view prefix="https://store-images.s-microsoft.com/";
            if (url.compare(0,prefix.size(),prefix)==0) return url;
        }
    return {};
}

std::optional<Catalog> ParseCatalog(std::string_view bytes) {
    const auto root = steam::json::Parse(bytes, {16 * 1024 * 1024, 8, 100000, 16384});
    if (!root || !root->is_object()) return std::nullopt;
    const auto* schema = root->Find("schema");
    const auto* account = Text(*root, "accountKey", 64);
    const auto* gamertag = Text(*root, "gamertag", 256);
    const auto* market = Text(*root, "market", 2);
    const auto* active = root->Find("subscriptionActive");
    const auto* games = root->Find("games");
    const auto* skipped = root->Find("skippedEntitlements");
    if (!schema || schema->AsUint64() != 1 || !account || account->size() != 64 ||
        !std::all_of(account->begin(), account->end(), [](char c) { return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'); }) ||
        !gamertag || !market || market->size() != 2 ||
        !std::all_of(market->begin(), market->end(), [](char c) { return c >= 'A' && c <= 'Z'; }) ||
        !active || !active->is_bool() || !games || !games->is_array() || games->array().size() > 10000 ||
        !skipped || !skipped->AsUint64() || *skipped->AsUint64() > 200000) return std::nullopt;
    Catalog result;
    result.account_key = *account;
    result.gamertag = *gamertag;
    result.market = *market;
    result.subscription_active = active->boolean();
    result.partial = *skipped->AsUint64() != 0;
    std::set<std::string> seen;
    for (const auto& row : games->array()) {
        const auto* product = Text(row, "productId", 12);
        const auto* title = Text(row, "title", 512);
        if (!product || !title || !seen.insert(*product).second) return std::nullopt;
        GameSummary game;
        game.id = GameId::Xbox(*product);
        if (!game.id.valid()) return std::nullopt;
        game.title = *title;
        if (!OptionalText(row, "developer", 512, game.developer) ||
            !OptionalText(row, "artwork", 4096, game.artwork_source) ||
            !OptionalText(row, "heroArtwork", 4096, game.hero_artwork_source)) return std::nullopt;
        const auto* description = row.Find("description");
        if (description && !description->is_null()) {
            if (!description->is_string() || description->text().size() > 8192) return std::nullopt;
            game.description = description->text();
        }
        constexpr std::string_view prefix = "https://store-images.s-microsoft.com/";
        if (!game.artwork_source.empty() && game.artwork_source.compare(0, prefix.size(), prefix) != 0) return std::nullopt;
        if (!game.hero_artwork_source.empty() && game.hero_artwork_source.compare(0, prefix.size(), prefix) != 0) return std::nullopt;
        result.games.push_back(std::move(game));
    }
    return result;
}
}  // namespace playden::stores::xbox
