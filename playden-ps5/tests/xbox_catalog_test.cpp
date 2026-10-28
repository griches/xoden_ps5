// SPDX-License-Identifier: GPL-3.0-or-later
#include "stores/xbox/catalog.h"
#include "stores/xbox/library_source.h"
#include <cassert>
#include <string>
#include <cstdio>
#include <vector>
#include <filesystem>
#include <thread>
#include <chrono>
#include <unistd.h>

namespace {
playden::LibraryEvent Wait(playden::stores::xbox::XboxLibrarySource& source, std::uint64_t request) {
    for (int i = 0; i < 2000; ++i) {
        for (auto& event : source.Pump(16)) if (event.request_id == request) return event;
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    assert(false); return {};
}
void ServiceChecks(const std::string& catalog) {
    char temp[] = "/tmp/playden-xbox-XXXXXX";
    assert(mkdtemp(temp));
    const auto path = std::filesystem::canonical(temp);
    playden::Storage storage(path.string());
    assert(storage.WriteAtomicPrivate("catalog.json", {catalog.begin(), catalog.end()}).ok());
    playden::stores::xbox::XboxLibrarySource source(path.string());
    const auto first = source.Refresh(0, 1);
    auto event = Wait(source, first);
    assert(event.kind == playden::LibraryEventKind::kComplete && event.games.size() == 1);
    assert(source.Snapshot() && source.Snapshot()->account_key == std::string(64, 'a'));
    source.Invalidate(2);
    assert(!source.Snapshot() && source.Refresh(0, 1) == 0);
    assert(storage.WriteAtomicPrivate("catalog.json", {'{'}).ok());
    event = Wait(source, source.Refresh(0, 2));
    assert(event.kind == playden::LibraryEventKind::kFailure && event.error == playden::steam::LibraryError::kProtocol);
    assert(storage.WriteAtomicPrivate("catalog.json", {catalog.begin(), catalog.end()}).ok());
    const auto clear = source.Clear(0, 3); assert(clear);
    source.Invalidate(4); source.Shutdown();
    event = Wait(source, clear);
    assert(event.kind == playden::LibraryEventKind::kCleared);
    assert(!std::filesystem::exists(path / "catalog.json") && !source.Snapshot());
    assert(source.Refresh(0, 4) == 0);
    std::filesystem::remove_all(path);
}
}

int main(int argc, char** argv) {
    using playden::stores::xbox::ParseCatalog;
    const std::string head = "{\"schema\":1,\"accountKey\":\"" + std::string(64, 'a') +
        "\",\"gamertag\":\"Example\",\"market\":\"GB\",\"subscriptionActive\":true,\"skippedEntitlements\":0,\"games\":[";
    const std::string game = R"({"productId":"9N5PKH0KW2L1","title":"Minishoot","developer":null,"artwork":null})";
    const auto parsed = ParseCatalog(head + game + "]}");
    assert(parsed && parsed->games.size() == 1 && parsed->subscription_active);
    ServiceChecks(head + game + "]}");
    assert(playden::ToKey(parsed->games.front().id) == "xbox-9n5pkh0kw2l1");
    const auto art = ParseCatalog(head + R"({"productId":"9N5PKH0KW2L1","title":"Example","artwork":"https://store-images.s-microsoft.com/poster","heroArtwork":"https://store-images.s-microsoft.com/wide"}]})");
    assert(art && art->games.front().hero_artwork_source == "https://store-images.s-microsoft.com/wide");
    assert(!ParseCatalog(head + R"({"productId":"9N5PKH0KW2L1","title":"Example","heroArtwork":"https://store-images.s-microsoft.com.evil/wide"}]})"));
    const auto hero_game=playden::GameId::Xbox("9N5PKH0KW2L1");
    const auto hero_response=[](const std::string& id,const std::string& images) {
        return "{\"Products\":[{\"ProductId\":\""+id+"\",\"LocalizedProperties\":[{\"Images\":["+images+"]}]}]}";
    };
    const std::string hero_images=R"({"ImagePurpose":"Poster","Uri":"//store-images.s-microsoft.com/portrait"},{"ImagePurpose":"TitledHeroArt","Uri":"//store-images.s-microsoft.com/titled"},{"ImagePurpose":"SuperHeroArt","Uri":"//store-images.s-microsoft.com/wide"})";
    assert(playden::stores::xbox::ParseHeroArtwork(hero_response(hero_game.value,hero_images),hero_game)=="https://store-images.s-microsoft.com/wide");
    assert(!playden::stores::xbox::ParseHeroArtwork(hero_response("9NF6XPSBTTGB",hero_images),hero_game));
    for (const auto* uri:{"https://store-images.s-microsoft.com.evil/wide","https://secret@store-images.s-microsoft.com/wide","http://store-images.s-microsoft.com/wide"}) {
        const auto images="{\"ImagePurpose\":\"SuperHeroArt\",\"Uri\":\""+std::string(uri)+"\"}";
        assert(!playden::stores::xbox::ParseHeroArtwork(hero_response(hero_game.value,images),hero_game));
    }
    assert(!playden::stores::xbox::ParseHeroArtwork(hero_response(hero_game.value,R"({"ImagePurpose":"Poster","Uri":"//store-images.s-microsoft.com/portrait"})"),hero_game));
    assert(!ParseCatalog(head + game + "," + game + "]}"));
    assert(!ParseCatalog(head + R"({"productId":"../escape","title":"Oops"})]})"));
    assert(!ParseCatalog(head + R"({"productId":"9N5PKH0KW2L1","title":"Oops","artwork":"https://store-images.s-microsoft.com.evil/a"})]})"));
    assert(!ParseCatalog(head + R"({"productId":"9N5PKH0KW2L1","title":"Oops","artwork":"https://secret@store-images.s-microsoft.com/a"})]})"));
    assert(!ParseCatalog(head + R"({"productId":"9N5PKH0KW2L1","title":"Oops\nInjected"})]})"));
    // A cache includes every game, not merely the first UI page.
    std::string many = head;
    for (int i = 0; i < 600; ++i) {
        if (i) many += ',';
        const auto number = std::to_string(100000000000ULL + i);
        many += "{\"productId\":\"" + number + "\",\"title\":\"Game " + std::to_string(i) + "\"}";
    }
    const auto full = ParseCatalog(many + "]}");
    assert(full && full->games.size() == 600 && full->games.back().title == "Game 599");
    if (argc == 2) {
        FILE* file = std::fopen(argv[1], "rb"); assert(file);
        std::vector<char> bytes(16 * 1024 * 1024 + 1);
        const auto size = std::fread(bytes.data(), 1, bytes.size(), file);
        assert(!std::ferror(file)); std::fclose(file);
        const auto actual = ParseCatalog(std::string_view(bytes.data(), size));
        assert(actual);
        std::printf("Validated console catalog: %zu games\n", actual->games.size());
    }
}
