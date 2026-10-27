# Game Pass compatibility

PC Game Pass / Microsoft Store editions tested with this project's PS5 app.
Last updated: **2026-10-27**. Current test console: firmware **13.60**, runtime
kit **0.13**. Dates and limitations below matter: a successful launch is not a
completed gameplay test, and an older imported install does not validate the
current direct-download flow.

[Open a compatibility report](https://github.com/griches/gamepass-ps5/issues/new?template=compatibility-report.yml),
including successful tests. Reports stay in this private repository. Access is
limited to people who can access the repository.

## Status

| Status | Evidence required |
| --- | --- |
| ✅ Works | A playable section was tested with the input method stated in the notes. |
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
| Call of the Elder Gods | 🔵 Launches | 0.13 | — | Gary reports successful launch after restarting and jailbreaking on 2026-10-27. Direct PS5 download completed; large-package memory and preparation recovery fixes applied. Registration stalled until reboot. Controller gameplay, saves and performance need a report. App source: `547a6c8`. [Record](xodus-games/GAMEPASS-PS5-RECORD.md#2026-10-27--user-confirmed-elder-gods-launch-after-reboot). |
| DOOM 64 | ⚪ Untested | — | — | Gary reported it installed. No sufficiently detailed launch or gameplay test is recorded. |
| Gears of War: E-Day | ❌ Broken | — | — | Earlier Mac-download/import attempt was followed by Gary's Easy Anti-Cheat / Wine-helper error reports. No successful gameplay recorded; exact failure cause and applicability to the current native flow are unverified. Test install was later removed at Gary's request. |
| Halo Infinite | ⚪ Untested | — | — | Download cancelled and partial files removed. No launch attempted; no separate campaign compatibility result. Anti-cheat discussion alone is not a console test. |
| Hollow Knight | 🔵 Launches | 0.13 | Controller menus | Historical Mac-prepared/imported Game Pass copy reached the signed-in main menu. Menu navigation verified; no recorded gameplay or save-slot acceptance. Removed at Gary's request. Needs retesting through the current native install flow. [Record](xodus-games/GAMEPASS-PS5-RECORD.md#console-launch-acceptance--26-october-2026). |
| Minishoot' Adventures | ✅ Works | 0.13 | Unity; controller | Native Xbox install reached signed-in menus and tutorial. Movement escaped the crystal; traversal, collision, damage and pause worked. Saved progress reloaded and quitting returned to the app. Shooting, extended play and a fresh install on the latest embedded downloader remain untested. [Gameplay](xodus-games/GAMEPASS-PS5-RECORD.md#gameplay-acceptance-on-the-native-xbox-install), [save/reload](xodus-games/GAMEPASS-PS5-RECORD.md#native-play-button-and-return-accepted). |
| MIO: Memories in Orbit | ⚪ Untested | — | — | Direct PS5 download and automatic D3D12/vkd3d runtime preparation completed on 2026-10-27. Home title registered while the app was running. Earlier pause/restart/resume and cancelled-partial cleanup tests passed. No gameplay test. |
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
