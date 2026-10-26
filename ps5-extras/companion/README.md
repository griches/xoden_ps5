# Xbox / Game Pass companion for Playden

Development build for Gary's PS5. The current interface runs on the Mac; it is
not yet an Xbox tab inside Playden. Microsoft sign-in stays in Xodus's native
browser window and Mac Keychain. The companion reuses Xodus Nexus's account
library, catalogue, entitlement and package-selection modules.

## Run

From the `ps5-homebrew` folder:

```sh
PS5_HOST=192.168.0.174 node --disable-warning=ExperimentalWarning xodus-games/ps5-extras/companion/server.mjs
```

You can also double-click `start.command`. Open <http://127.0.0.1:47831> on this Mac. Node 24 or newer is required.
The service listens only on loopback and requires a per-process token for API
requests. Do not expose it through a public tunnel.

1. The existing Xodus account is restored. **Sign in with Microsoft** opens
   Xodus's normal Microsoft sign-in window if another sign-in is needed.
2. **Refresh library** fetches account entitlements from Microsoft. Only games
   classified as Game Pass access are listed. Remaining pages load automatically; the page marks when the full library is
   available for search. Partial results are shown while loading.
3. **Choose package** resolves the offer's actual PC packages. Base games with
   Windows.Desktop MSIXVC packages are selectable. Package compatibility is not
   a claim that the game runs on PS5.
4. Close Playden and any running game, then choose **Install to PS5**. The Mac
   downloads the game, checks access again, prepares the Xbox runtime, transfers
   the files and registers an Imported game in Playden. Leave the Mac awake.
5. Open Playden and select the imported game. Playden prepares its normal
   private bottle and PS5 title. **Runtime finalization is still manual:** after
   preparation, close the game and run `ps5_xbox.py setup INSTALL_ID` from the
   parent directory (with `PS5_HOST` set). This copies the Xbox runtime into that
   bottle's Windows system folder and verifies it. Then launch again. Placing
   the DLLs only alongside the executable did not load the bridge in the console
   test. The shared runtime kit is unchanged.

An install requires a recent active PC Game Pass or Ultimate summary, a fresh
per-game Game Pass entitlement, a supported selected package, and successful
Xodus licensing. Unknown membership, revoked or expired access, account changes
and unsupported packages fail closed. These are installation checks; this build
does not add a subscription check to Playden's launch screen.

## Scope and recovery

- The first transport supports internal storage, x64 Windows games and one
  non-development executable in `MicrosoftGame.config`.
- The shared PS5 sign-in is refreshed from the Mac's Xodus Keychain during an
  installation. Token contents never go to the browser.
- Downloads live under the `downloadRoot` in `config.json` (currently `/Volumes/PS5/XodusDownloads`), in `<product ID>-<content ID>`. `XBOX_DOWNLOAD_ROOT` overrides that setting. The configured root must already exist; an absent external drive fails rather than falling back to the internal disk. Interrupted downloads
  are kept. A completed preparation checkpoint avoids repeating downloads on a transfer retry. Uploads use `/data/playden-import/Xbox-<product ID>.partial`.
- Upload retries hash-check staged files and reuse matching bytes. Transfers use four independent FTP sessions.
- The final folder is published only after upload checks. Runtime DLLs are
  verified by reading them back; game files are checked for complete size.
- Playden must remain closed through publication. Its imports and settings are
  re-read before publication; concurrent changes abort the operation.
- Private metadata writes are atomic and keep the first `.xodus-bak` backup.
  Existing import/settings entries are preserved. A completed destination is
  never overwritten automatically. A failure after its final rename needs
  inspection before retry; do not delete a game folder containing saves.
- No pause, cancellation UI, resumable queue persistence, uninstall or native Xbox tab
  yet. Starting another install while one is running is refused.
- A second test copy is explicitly named **Hollow Knight (Xbox companion test)**;
  the original Hollow Knight install is kept intact.

## Dependencies and provenance

- `../../../xodus-nexus`, commit `3006f141331cf5579d12261e4ac80ba7c2ffcf74`.
- `../../../xodus-ps5-backend`: isolated Xodus worktree at
  `53ea781dcd073e31d7ea68e894134e9c959bff04`, with all 52 patches from Nexus's
  `assets/backend-source.lock.json` applied after SHA-256 verification.
- Backend built with `cargo build --release -p xodus-cli`; Rust 1.99.1 and
  Homebrew protobuf 36.2. The native webview feature is enabled for Mac sign-in.
- A local `prepare-ps5` backend command supplies retry-safe image preparation, based on the earlier PS5 command. It requests the Microsoft content license, preserves encrypted originals and atomically publishes prepared images.
- Existing runtime DLLs, exporter and `uidcp` under `../` supply console setup.
- `nexus-loader.mjs` uses Node's TypeScript transformer to load the upstream
  modules directly, without installing or starting Electron.
- Xodus is GPL-3.0; Xodus Nexus is MIT. Keep their source and license files with
  any distribution. Nothing from this work has been published.

## Verification

```sh
node --disable-warning=ExperimentalWarning --test xodus-games/ps5-extras/companion/backend.test.mjs
python3 -m unittest discover -s xodus-games/ps5-extras/companion -p 'test_*.py'
python3 xodus-games/ps5-extras/companion/install_ps5.py --host 192.168.0.174 --source xodus-games/HollowKnight --product 9MW9469V91LM --title 'Hollow Knight' --check
```

The live account, subscription, library and Hollow Knight package preflight
were verified on 26 October 2026. The private-file payload was built and tested
on the console with a harmless 0600 test file. See `../../GAMEPASS-PS5-RECORD.md`
for the latest console acceptance results.

The Nexus-patched CLI suite passed 110/111 on this Mac. The remaining test
creates `Game.exe` and `GAME.EXE` as separate files, which this case-insensitive
filesystem cannot represent. No production source was changed to hide that
failure. Playden source and its pinned runtime kit are unchanged.

## Console acceptance (26 October 2026)

The separate Hollow Knight import prepared as `FAKE78003`, reached its main
menu with the Xbox profile displayed, and issued a fresh Microsoft token.
Remote Play menu input worked. This required manual per-bottle runtime setup;
automating that step remains outstanding. No gameplay/save-slot test or fresh
Microsoft download was performed in this acceptance run. The installer now
sets the program explicitly and uses `library_overrides`; the former
`registry_dll_overrides` field is inactive in the installed Playden version.

The optional live viewer runs with `node remote-view.mjs` at
<http://127.0.0.1:47832>, while DevLoop `ps5rp.py serve` is running.
`serve --input` enables explicit local `tap` commands for testing. The viewer
itself has no controls. A physical pad taking over the same console user may
interrupt the Remote Play session.

Installation status also has a read-only view at <http://127.0.0.1:47833>
(`node progress-view.mjs`). Terminal job status is retained in `.last-job.json`;
this does not resume an interrupted job.
