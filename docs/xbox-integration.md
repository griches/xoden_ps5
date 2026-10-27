# Xbox integration work record

User objective: controller-driven Xbox Game Pass browsing, search over the full
library, install progress and game launch in Playden, without a Mac service or
manual compatibility edits. Work is authorized to continue unattended; commit
regularly. No publishing/pushing requested.

## Proven baseline

A Windows x64 Xodus worker executes inside the existing PS5 Wine kit. It reads
the private console sign-in, verifies a live subscription, reads Store
entitlements, downloads Microsoft packages directly to console storage and
prepares licensed images. Minishoot downloaded completely (417684235 bytes),
prepared and launched to its signed-in main menu. See the sibling
`xodus-games/GAMEPASS-PS5-RECORD.md` for evidence and install identities.

Modern Unity's API name strings are not evidence that the game's shaders were
built for that API. Defaults now let modern Unity select its built renderer,
while installing vkd3d-proton when the kit supports it. Legacy D3D9 defaults
are retained. This still needs acceptance with fresh generated settings.

## Implementation direction

- Add an Xbox store identity and library source to the existing registry. The
  native UI uses a bounded, complete cached catalogue; search includes every
  loaded title, with explicit refresh status. Never expose credential tokens.
- A structured private job request selects refresh or install, with a unique
  request ID. The console worker verifies subscription and game entitlement
  afresh before choosing a PC package, downloads, prepares and writes a bounded
  result atomically. No signed CDN URLs or credentials enter UI result files.
- A Wine utility is a separate PS5 title because native Playden cannot spawn
  processes. Supply a progress window and a console handoff/return mechanism;
  do not use the Mac companion to execute jobs. Consume only matching results.
- Native Playden records downloaded game locations and uses its existing runtime
  preparation/publication path. Add per-bottle Xbox DLL setup automatically.
- Preserve account separation and existing saves. Failed/interrupted jobs must
  be recoverable and must not publish a ready game before all steps succeed.
- Test the native full-library/search/install flow and fresh-game automatic
  renderer selection on the console, not only host fixtures.

## Current repositories

- Playden branch `xbox-store`, fast-forwarded to upstream 9aae269 (v0.7.2).
- Xodus backend branch `ps5-account-backend`, checkpoint af9f0da.
- Supporting tooling in sibling `xodus-games`, source-only local repository,
  initial checkpoint c9dbf9b. Downloads, private config and binaries ignored.

Latest Playden changes its launcher title to PPSA97031. Account data and kit
remain under the shared Playden data root. Deployment must account for the old
FAKE77031 launcher and keep rollback available. Do not edit pinned kit hashes.

## 2026-10-27 catalog checkpoint

- Backend `6e9893f` implements bounded account-bound jobs. On-console refresh
  completed with 669 games and an active subscription; the actual 328610-byte
  snapshot validates in the native reader.
- Native reader and asynchronous registry source committed as `ce564da`.
  Full host suite passes 149/149; PS5 package builds successfully.
- Build scripts explicitly retain FAKE77031; this deployment uses the existing
  launcher identity. Verified raw backup retained in sibling xodus-games.
- Console FTP transparently decodes SELF on retrieval. Ordinary byte comparison
  fails even for correct uploads. The uidcp get-raw envelope preserves exact
  on-disk bytes; use it for executable backup and verification.
- Installed catalog checkpoint includes an explicit DevLoop frame capture hook.
  Native UI console acceptance follows; install handoff/setup remains unfinished.
