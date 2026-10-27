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

## 2026-10-27 native partial-download management

The Xbox details page now distinguishes resumable partial downloads and offers
Delete partial download with a confirmation. Cleanup runs on the Xbox service
worker through ManagedDirectory, rejects prepared games and unreadable prepared
manifests, and never follows symlinks or touches bottle/save roots. Both streaming
journals and early metadata-cache files identify incomplete downloads. Markers
are removed last so ordinary failures remain retryable. Matching current jobs
receive cancelled status; older orphan cleanup leaves newer job status intact.

This is an incremental implementation on `xbox-store`, not the completed goal.
Downloads still hand off to the Windows worker. Outstanding work includes native
in-app downloading/progress/cancellation, durable recovery if cleanup is interrupted at its final
step, and console acceptance of the new UI. Do not describe this as autonomous
cancellation cleanup or a completed native Xbox downloader.

Native downloader feasibility probe (not shipped): installed the Rust
`x86_64-unknown-freebsd` standard library and built a tiny static library doing
file metadata and a worker thread. Linking with the PS5 payload SDK, even with
`--gc-sections`, fails on `dl_iterate_phdr`, `pthread_setname_np`, and seven unwind
symbols. Probe source/logs are under `/tmp/xbox-native-probe`. This establishes
that the existing Rust downloader cannot simply be linked unchanged; it does
not establish that ABI shims alone would be sufficient. Thread/file ABI and
Tokio/network/TLS compatibility must be established before a native backend can
replace the current Wine handoff. No probe was executed on the console.

Follow-up probe: linking the SDK's libunwind resolves the unwind symbols;
using its `pthread_set_name_np` and a probe-only empty backtrace module iterator
gets past symbol resolution. The SDK payload link then fails on Rust standard
library thread-local storage (`STT_TLS` without `SHF_TLS`). No compatibility shim
has been added to product code. A title-specific TLS approach or custom Rust
standard library remains necessary; treating a successful archive compilation
as a working native downloader would be incorrect.

The isolated probe now **links** when Rust std is rebuilt using nightly
1.101.0-nightly (2026-10-26), `-Zbuild-std=std,panic_abort`, and
`RUSTFLAGS=-Ztls-model=emulated`, plus the SDK's libunwind and the two probe-only
adapters above. Artifact: `/tmp/xbox-native-probe/probe.elf`. This is compile/link
evidence only. The default Rust toolchain and product dependencies are unchanged.
Next: establish the PS5/FreeBSD file/thread/network ABI before attempting the
Xodus/MSIXVC libraries; verify inside a native title, then expose bounded progress
and cancellation to Playden. Do not deploy this probe over an active game.

Cleanup checkpoint validation: full host suite 151/151 passed in 132.92 s
(`/tmp/playden-xbox-cleanup-tests-verified.log`); native package build passed
(`/tmp/playden-xbox-cleanup-ps5-final.log`). Controller test exercises confirmation,
correct game identity and disabling Install during deletion; service tests cover
orphan removal, prepared/corrupt-manifest refusal, symlink protection, first-job
metadata cleanup and persisted cancelled status on reload. Console acceptance
is still pending at this checkpoint.

## Home-screen artwork requirement (Gary, 2026-10-27)

Each installed Xbox game's PS5 home-screen title must include `icon0` and `pic0`
(the tile icon and background), similar to the presentation produced by Playden.
Generate these from the game's downloaded catalogue/store artwork during title
preparation. Reuse the downloaded assets rather than generic Xbox placeholders;
size/crop/encode them to the existing title publisher's required formats. This
requirement applies to our current Playden branch and any later standalone app.
Implementation and console verification remain outstanding; Gary requested that
it be recorded while download/cleanup work continues.

## Deployment incident and cleanup recovery follow-up

The 2026-10-27 deployment attempt lost contact during the UID helper raw backup
read. FTP, ELF loader, status and DevLoop endpoints subsequently stopped
responding, and Gary reported a crashed console. The traceback is in
`/tmp/playden-xbox-cleanup-deploy.log`; the requested backup directory was never
created. Source control flow puts all package replacement after the backup scan,
so the failed attempt did not reach that replacement loop. Directory creation or
mode checks preceded it. Root cause is unknown; the backup helper is the last
observed operation, not a proven cause. Do not retry payload deployment or run
native probes until the incident has been investigated. Console operations were
stopped; local development continues.

Cleanup recovery now renames an admitted partial to `.delete-xbox-<product>` and
syncs the parent before deleting its contents. This canonical quarantine remains
discoverable after the last marker is removed. Cancellation state is persisted
before removing the empty quarantine. New installs refuse a pending deletion;
cleanup refuses a collision with a new canonical download. Prepared-game and
manifest-validation guards remain in place. A restart fixture covers retrying
an empty quarantine after interruption.

Recovery follow-up: Gary rebooted the console. Read-only probes found FTP2121
and ELF loader9021 responding; helpers9111/9112 were absent after reboot. FTP
listing shows the existing eboot (31,382,124 listed bytes, Oct 27 06:27), with no
staged/old eboot beside it. Last uidcp command was the first raw eboot backup read;
no uidcp log or deploy-raw file was listed after reboot. No new payload or launch
command was sent during these checks. File content/version is not yet verified.

Recovery change validation: 151/151 host tests pass in 135.18 s; native PS5 build
passes. Logs: `/tmp/playden-cleanup-recovery-tests.log` and
`/tmp/playden-cleanup-recovery-ps5.log`. These changes have not been deployed.
