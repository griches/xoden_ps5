// store_art.cpp - A game's artwork addresses, from its store.
// Copyright (c) 2026 Playden contributors
// SPDX-License-Identifier: MIT
#include "homebrew/store_art.hpp"

#include "homebrew/simulation.hpp"

namespace playden::homebrew {
namespace {

std::string SteamArt(const GameId& game, const char* name) {
  if (!game.steam_app()) return {};
  return "https://cdn.cloudflare.steamstatic.com/steam/apps/" + ToKey(game) + "/" + name;
}

}  // namespace

std::string CoverUrl(const GameId& game) { return SteamArt(game, "library_600x900.jpg"); }
std::string HeroUrl(const GameId& game) { return SteamArt(game, "library_hero.jpg"); }
std::string LogoUrl(const GameId& game) { return SteamArt(game, "logo.png"); }
std::string CoverUrl(const Game& game) {
  if (game.id.store == Store::kXbox && !game.artwork_source.empty())
    return game.artwork_source.substr(0, game.artwork_source.find('?')) + "?w=600&h=900&q=85";
  return CoverUrl(game.id);
}
std::string HeroUrl(const Game& game) {
  if (game.id.store == Store::kXbox && !game.artwork_source.empty())
    return game.artwork_source.substr(0, game.artwork_source.find('?')) + "?w=1920&h=1080&q=85";
  return HeroUrl(game.id);
}
std::string LogoUrl(const Game& game) { return LogoUrl(game.id); }

}  // namespace playden::homebrew
