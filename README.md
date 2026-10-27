# Game Pass on PS5

Private development repository for a PS5 app that browses PC Game Pass, downloads
games directly to the console, and prepares individual PS5 Home entries.
The app is currently based on Playden; its separate identity and packaging are
still in progress. A working console account is currently required: fresh PS5
sign-in is unfinished.

**[Game compatibility](COMPATIBILITY.md)** ·
**[Remaining work](ROADMAP.md)** ·
**[Development and source provenance](DEVELOPMENT.md)** ·
**[Detailed work record](xodus-games/GAMEPASS-PS5-RECORD.md)**

## Current results

- Full Game Pass catalogue browsing and search, persistent cover cache and green UI.
- Direct console downloads, package/size review, pause/resume and cancel cleanup.
- Gameplay verified for Minishoot' Adventures and A Game About Digging A Hole.
- Call of the Elder Gods launch reported by Gary after restarting and jailbreaking.
- Fresh console sign-in, download efficiency and registration recovery need work.

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
