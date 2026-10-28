# XoDen

<img src="playden-ps5/sce_sys/icon0.png" alt="XoDen's original green portal icon" width="128">

Private development repository for a PS5 app that browses PC Game Pass, downloads
games directly to the console, and prepares individual PS5 Home entries.
XoDen combines Xodus authentication and downloads, the Nexus browser sign-in
bridge, and Playden's console interface and runtime preparation. Each component
retains its source attribution and license.

**[Game compatibility](COMPATIBILITY.md)** ·
**[Remaining work](ROADMAP.md)** ·
**[Development and source provenance](DEVELOPMENT.md)** ·
**[Detailed work record](xodus-games/GAMEPASS-PS5-RECORD.md)**

## Current results

- Full Game Pass catalogue browsing and search, persistent cover cache and green UI.
- Direct console downloads, package/size review, pause/resume and cancel cleanup.
- Gameplay verified for Minishoot' Adventures and A Game About Digging A Hole.
- Fresh Microsoft sign-in, verified session save and automatic library refresh
  completed in one menu attempt on the PS5. Call of the Elder Gods game sign-in
  and visibility of existing saves were also confirmed by Gary.
- Download efficiency, registration recovery, other games and session-expiry
  behavior still need work; see the roadmap for the remaining acceptance checks.

The current sign-in helper requires **Google Chrome installed on a computer on
the same network**. Its local page can open in Safari or another default browser;
Microsoft approval opens in a separate temporary Chrome window. XoDen keeps one
Microsoft sign-in flow for the library, licensing and game authorization.

The original icon, background and generation prompts are kept in
[the branding folder](playden-ps5/assets/branding/xoden-prompts.md).

## Source layout

| Directory | Purpose |
| --- | --- |
| `playden-ps5/` | App UI, download manager, runtime preparation and PS5 registration. |
| `xodus-ps5-backend/` | Active Rust backend and embedded native PS5 downloader. |
| `xodus-games/` | Integration tools, legacy Mac companion and chronological work record. |
| `xodus/` | Earlier CLI changes and in-process Xbox runtime-wrapper source; retained for provenance. |
| `xodus-nexus/` | Pinned upstream submodule used by the legacy companion. |

Clone with `git clone --recurse-submodules https://github.com/griches/gamepass-ps5.git`.
Account files, game downloads, local console configuration, build caches and
deployment backups are not included. Each component retains its original license
and attribution; see [development notes](DEVELOPMENT.md).

Compatibility reports belong to **this repository's Issues**. This compatibility
list contains our PC Game Pass tests; Steam results in the imported Playden source
are not evidence that the Game Pass edition works. App-side ratings currently
remain local; automatic submission to this repository has not been implemented.
