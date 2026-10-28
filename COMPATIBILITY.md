# Game Pass compatibility

PC Game Pass / Microsoft Store editions tested with this project's PS5 app.
Last updated: **2026-10-28**. Current test console: firmware **13.60**, runtime
kit **0.13.1** (native launcher patch over 0.13). Dates and limitations below matter: a successful launch is not a
completed gameplay test, and an older imported install does not validate the
current direct-download flow.

**[Report a result — no file editing](https://github.com/griches/xoden_ps5/issues/new?template=compatibility-report.yml)** ·
**[Propose an edit to this list](https://github.com/griches/xoden_ps5/edit/main/COMPATIBILITY.md)**

The easiest route is the report form: enter the game and what worked or failed;
a maintainer reviews the report and updates the table. The edit link opens
GitHub's file editor; submit your change as a pull request for review.
Neither route changes the list automatically. Successful reports are welcome.
Anyone with a GitHub account can submit a report or propose an edit;
maintainers review changes before updating the list.

## Status

| Status | Evidence required |
| --- | --- |
| ✅ Works | A playable section was tested or confirmed by a user; notes identify the report and input/coverage when supplied. |
| 🟡 Partial | Gameplay was tested, with a specific remaining problem. |
| 🔵 Launches | The game opened; gameplay coverage is insufficient for Works. |
| ❌ Broken | An attempted launch or gameplay test failed; the failure is recorded. |
| ⚪ Untested | No usable launch or gameplay result is recorded. Downloading alone does not qualify. |

Graphics settings are listed only when established for that exact PC edition.
Unknown values are left as **—**. Steam compatibility reports are not copied into
this list.

## Games

| Game | Status | Kit | Graphics / input | Tested result and limitations |
| --- | --- | --- | --- | --- |
| A Game About Digging A Hole | ✅ Works | 0.13 | Unreal WinGDK; controller | Direct in-app PS5 download and automatic preparation. Menu, movement, looking and digging verified on 2026-10-27. Automatic WinGDK shipping executable selection fixed. Save/reload and extended play not tested. The earlier control problem was the test controller retaining control. [Record](xodus-games/GAMEPASS-PS5-RECORD.md#2026-10-27--gameplay-proof-and-accountdownload-ui-work). |
| Balatro | ❌ Broken | 0.13.1 | — | PC Game Pass `9PK087LNGJC5`, correct package executable `love.exe`. Existing logs show early exit during runtime initialization before any game frame; exact cause remains unproven. Installed Xbox runtime DLLs match the bundled versions, including the expected windows.web marker patch. Detailed startup trace pending. |
| BLiNX: The Time Sweeper | ❌ Broken | 0.13.1 | EmuMenu default; direct Emu.exe tested with DXVK + vkd3d-proton | PC package `BRG51C5MWFSG`. .NET/WinUI launcher fails Windows App Runtime 1.8 bootstrap with `0x80004001`. Direct `Emu.exe` bypasses the menu, then exits with “Failed to allocate guest address space” and exception `c0000409`, before rendering. Original saved settings and live profile restored and independently verified at revision 2. No gameplay reached. |
| Blood Dungeon | ✅ Works | 0.13 | DXVK; controller through Remote Play | Direct in-app PS5 download, setup and native tile registration verified on 2026-10-28 (`9MSVBF0KZFVW`). Signed-in menu, opening playable area, movement, jumping, pause and menu navigation checked; audible audio telemetry and clean in-game Quit to desktop recorded. Brief test only; save/reload and extended play untested. Close XoDen to let PS5 add the tile, then reopen. |
| Call of the Elder Gods | ✅ Works | 0.13 (earlier test) | — | Gary confirmed working on 2026-10-28. Earlier direct PS5 download and launch were recorded after a registration/reboot recovery. Latest graphics/input settings, save/reload and extended-play details were not supplied. |
| Choo-Choo Charles | ✅ Works | 0.13.1 | DXVK; Direct3D 12: vkd3d-proton | PC Game Pass `9P17KJLSCR59`, `Obscure/Binaries/WinGDK/Obscure-WinGDK-Shipping.exe`. The original default exited with “D3D12 is not supported!” while loading Wine’s builtin d3d12.dll. Gary confirmed working on 2026-10-28 after setting Direct3D 12 to vkd3d-proton. Source defaults now select it automatically for the next app release; 0.1.0 users must set it manually. Save/reload and extended-play coverage were not supplied. |
| Donut County | ✅ Works | 0.13.1 | — | PC Game Pass `9N6V2181GHLM`. Gary confirmed working on 2026-10-28 after native download, preparation and Home registration. The earlier failure after about 1.95 MB of metadata was a Microsoft content-licensing device-limit rejection; a later retry acquired the licence. Detailed settings, save/reload and extended-play results were not supplied. |
| DOOM 64 | ✅ Works | — | — | Gary confirmed working on 2026-10-28. Exact runtime-kit version, installation method, graphics/input settings and save/reload coverage were not supplied. |
| Gears of War: E-Day | ❌ Broken | — | — | Earlier Mac-download/import attempt was followed by Gary's Easy Anti-Cheat / Wine-helper error reports. No successful gameplay recorded; exact failure cause and applicability to the current native flow are unverified. Test install was later removed at Gary's request. |
| High on Life 2 | ❌ Broken | 0.13.1 | DXVK / DX11 and vkd3d-proton / DX12 tested | PC Game Pass `9NF6XPSBTTGB`. Direct 91.1 GB download, native preparation and Home registration succeeded. DX11 later crashed with an RHI-thread access violation; DX12 reported unsupported DirectX 12. No working gameplay result. A successful installation does not establish runtime compatibility. |
| Halo: Campaign Evolved | ❌ Broken | 0.13.1 | DXVK + vkd3d-proton 3.0.1; default and NVIDIA vendor reporting | Direct Game Pass download and title setup complete (`9N683TDT5M7R`, `FAKE78004`). Launch is blocked by “Your GPU or driver isn’t supported.” NVIDIA reported-vendor test reproduced the same dialog on 2026-10-28; original GPU setting restored and republished. No gameplay reached; incompatible with the current runtime, exact rejected capability not identified. |
| Halo Infinite | ⚪ Untested | — | — | Download cancelled and partial files removed. No launch attempted; no separate campaign compatibility result. Anti-cheat discussion alone is not a console test. |
| Hollow Knight | ✅ Works | 0.13 (earlier test) | Controller menus (earlier test) | Gary confirmed working on 2026-10-28. The earlier Game Pass menu test used a Mac-prepared/imported copy; the latest installation route, settings and save/reload coverage were not supplied. |
| LIMBO | ✅ Works | 0.13.1 | DXVK (D3D9); native d3dcompiler_43 | PC Game Pass `9PNG83MDD6SH`, `Limbo.exe`. Gary confirmed it works on 2026-10-28 after fixing E5000 shader compilation with Microsoft's native compiler. Updated XoDen bundles the verified Microsoft DLL, installs it only into this game's bottle and applies the default override to new and older generated recipes. Automatic DLL installation and a fresh title-screen launch verified after removing the manual DLL/user override; save/reload and extended play not recorded. |
| Minishoot' Adventures | ✅ Works | 0.13 | Unity; controller | Native Xbox install reached signed-in menus and tutorial. Movement escaped the crystal; traversal, collision, damage and pause worked. Saved progress reloaded and quitting returned to the app. Shooting, extended play and a fresh install on the latest embedded downloader remain untested. [Gameplay](xodus-games/GAMEPASS-PS5-RECORD.md#gameplay-acceptance-on-the-native-xbox-install), [save/reload](xodus-games/GAMEPASS-PS5-RECORD.md#native-play-button-and-return-accepted). |
| MIO: Memories in Orbit | ✅ Works | 0.13 (earlier test) | D3D12/vkd3d; controller menus (earlier test) | Gary confirmed working on 2026-10-28. Earlier native PS5 download, automatic preparation, Home registration and menu/settings navigation were recorded. Latest settings, save/reload and extended-play coverage were not supplied. |
| 1000xRESIST | ⚪ Untested | — | — | PC package/size picker tested at 8.0 GB; cancelled before downloading. No launch test. |

## Reporting a test

Use the report form and include the exact PC edition/product ID, app commit,
runtime kit, console firmware, installation method, graphics settings, executable
choice and controller/input setup. State what you actually tested: menus,
movement, combat/actions, save/reload, and the approximate session length.

For a failed test, include the error and the last working step. Redact account
details, tokens, signed download links and console addresses from logs. Screenshots
are useful when available. A maintainer reviews reports before changing this table;
an Issue does not automatically establish compatibility.

## Current installation caveat

The loader can stall while adding a completed download to PS5 Home. Elder Gods
launched after Gary restarted and jailbroke again. A same-version live loader
reload attempt left the console unreachable; it is not a validated recovery
procedure. This installation failure is recorded separately from game runtime
compatibility. See the [work record](xodus-games/GAMEPASS-PS5-RECORD.md#completed-download-registration-acceptance-and-loader-recovery-result).

On 2026-10-28, Blood Dungeon completed registration after closing XoDen and
returning to PS5 Home; reopening XoDen reconciled it to Ready without a reboot.
This verifies recovery for that install, not every older registration failure.
