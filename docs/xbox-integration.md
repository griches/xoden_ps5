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

## FTP-only executable backup and verification

Upstream ftpsrv documents a connection-local `SELF` toggle (default on):
https://github.com/ps5-payload-dev/ftpsrv/blob/master/README.md . The recovered
console advertises SELF; on a fresh FTP connection it replied
`226 SELF transfer mode disabled`. Reading eboot then produced 30,810,283 raw
bytes with SHA256
`7beb1b2ac5cace748d20f9da6a2abaf430cd909636ef7a3c490f47bf5057544a`, exactly the
previous verified deployed build. This verifies that executable was unchanged.
No ELF payload was needed. Raw read saved as `/tmp/playden-recovered-eboot.bin`.

`console_title_deploy.py` now negotiates raw transfers on its fresh connection
before any package operations and refuses an unconfirmed SELF-mode reply. It
also accepts ftpsrv's successful 226 DELE response. Five regression cases cover
toggle confirmation, refusal, ordinary FTP, FEAT errors and delete responses;
they are registered as `console_title_deploy` in ctest. Full suite passes 152/152.
Use this path instead of the old `/tmp/deploy-playden-*.py` UID payload wrappers.
Console publication and cleanup UI acceptance are still outstanding.

Native library probe continuation: the existing MSIXVC library and its
Tokio/reqwest dependencies cross-compile to an x86_64 FreeBSD static archive with
emulated TLS. The probe needed a declaration of `getentropy` for aws-lc's C
sources; no entropy implementation or working TLS connection has been supplied
or claimed. Isolated fixture: `/tmp/xbox-native-msixvc-probe` (not product code).
The probe resolved reqwest 0.13.5 rather than using the backend's lockfile; final
integration must use pinned/reviewed inputs.

ABI audit found `sizeof(struct stat)` is 120 in the PS5 SDK versus 224 in default
Rust libc. These cannot be used interchangeably. Selecting
`--cfg libc_unstable_freebsd_version="11"` currently fails rebuilding Rust std at
`std/src/sys/fs/unix.rs:954` because dirent d_fileno is u32 while std expects u64.
An isolated/custom standard-library adaptation or an explicit C IO bridge is
needed before console execution. Do not treat the successful default-target
archive build as a runnable PS5 backend. Current log:
`/tmp/xbox-native-msixvc-probe/build-freebsd11.log`.

## Verified FTP-only retry and native parser checkpoint

After Gary requested a retry, the FTP-only deploy succeeded. Two files changed:
eboot.bin (SHA prefix `d2891aad89b4`) and playden-package.identity. Exact raw
read-back passed; backup is in sibling
`xodus-games/console-backups/20261027-xbox-cleanup-ftp`. No UID backup payload was
used. The existing ps5vkctl control helper was restored, reported idle, and
launched FAKE77031 successfully. Native screenshot `/tmp/playden-cleanup-startup.png`
shows the home screen and existing Doom 64/Minishoot entries. The capture was
retrieved and removed from the title's developer capture directory through FTP;
no Remote Play session or virtual pad was opened. The update boots; actual
cancel/delete UI acceptance is still outstanding.

Backend commit `c72ef04` adds `crates/xodus-ps5-native`, a versioned bounded C API
for XVD package header inspection, three Rust tests, an executable host C ABI
test, and an isolated PS5 build script. Host Rust and C tests pass; the C test
links to a PS5 executable with the adapted standard library. The build script
reproduced that result from a fresh directory with the workspace lockfile.
Fifteen file-stat field offsets and the 120-byte total match SDK declarations.
No native probe has been executed on the console. No networking, account refresh,
license resolution, extraction or progress/cancellation API is implemented by
this new crate yet. Installed downloads still use the existing Windows worker.

## Console cleanup acceptance: real interrupted download

- Selected A Game About Digging a Hole (9NGLST31DG26) in the native Xbox catalogue
  and pressed Install. Worker displayed 10.9%, 40.4 / 369.4 MB, 9.0 MB/s when
  captured (`/tmp/xbox-cleanup-download.png`). Interrupted this test worker;
  Playden returned automatically with an interrupted-operation notice.
- Native game details offered Resume download and Delete partial download
  (`/tmp/playden-partial-details.png`). Selected Delete, verified its confirmation
  (`/tmp/playden-delete-confirm.png`), then confirmed with the controller.
- Before deletion, FTP listed xbox-9nglst31dg26 alongside the two prepared game
  folders. After deletion, that partial folder and its quarantine were absent;
  Minishoot's xbox-9n5pkh0kw2l1 and Doom 64's xbox-9mxnd4pqlk3w remained.
- The UI disabled install/delete while working and returned to Install with no
  Delete action (`/tmp/playden-delete-finished.png`). This verifies actual native
  deletion on the console, not just a host fixture. Resume was offered but was
  not exercised in this run; power-loss recovery remains host-fixture evidence.
- Virtual pad was closed successfully after the check. Playden remains open on
  the test game's details. Remote Play was not used. No test download remains.

## Native in-app engine integration (built, not deployed)

The Xbox service now has an optional native runner. In this mode Install stays
in Playden, progress appears in the existing Downloads page and game details,
and Cancel uses the existing confirmation. It asks the native engine to stop,
waits for its runtime/file IO to drain, then reloads partial/prepared admission
and performs the existing managed cleanup. A late cancel never deletes a game
that has already reached the prepared manifest. Shutdown preserves resumable
partials. Unsupported pause controls are omitted for native Xbox jobs. Steam
snapshot refreshes preserve the active Xbox download row.

The backend is in sibling xodus-ps5-backend on ps5-account-backend:
870141a shared engine/callbacks, cf18b1b IO-draining lifecycle, 8cf3838 C polling
ABI/TLS/platform adapters, 67e2073 batched download writes. The native engine
keeps existing subscription/entitlement checks and uses the existing private
console credential store. Internal HTTP clients use Playden's packaged CA
bundle with certificate verification enabled. New sign-in UI is not implemented.

Build the backend with its isolated build_probe.py --job-engine, then set both
PLAYDEN_XBOX_NATIVE_SOURCE (absolute crate directory) and
PLAYDEN_XBOX_NATIVE_LIBRARY (absolute adapted static archive) when running
scripts/build-homebrew-ps5.sh. Ordinary builds retain the existing worker.
The build script clears these options when the environment variables are absent,
so an old CMake cache cannot silently enable experimental native downloads.

The full native PS5 package builds locally: eboot.bin 46871649 bytes, SHA256
57289e5467215a8450e5cf46feea8441f8cddb51a044644be6aa04e828121ec8. Host suite
152/152 passed (137.99 seconds), including a simulated cancellation that keeps
partial content until the runner acknowledges drained writes. The added UI test
also passes: progress survives a Steam refresh and cancel requires confirmation.
The C lifecycle test passes on the host; native console runtime/download
acceptance remains outstanding. No new package or Rust probe was deployed.

Native-title linking additionally supplies SDK-compatible dirfd and reports
minherit as unavailable; AWS-LC handles that optional fork-detection failure.
The five optional AWS-LC allocator hooks are explicitly resolved to null,
preserving its normal allocator and zeroization, rather than importing nonexistent
SDK hooks. Existing runtime kit, libc, compiler-rt and submodule pins are intact.

Gary reported 1-9 MB/s on wired gigabit internet. Inspection found no default
bandwidth cap and four concurrent files. The transfer loop issued a write per
4 KiB decrypted page. Backend 67e2073 batches these into 256 KiB writes, while
flushing before every durable checkpoint. A 1 MiB plus 123-byte fixture uses
5 writes instead of 257, preserves exact content and checks that the tail is
written before its checkpoint. Transfer tests: 49 pass, 2 pre-existing ignored;
shared engine: 74 pass; native lifecycle/boundary: 5 pass. This is not a measured
console speed gain, and the active download was not restarted to apply it.
A wired connection's negotiated link rate has not been verified.

Console observation during this work changed from worker FAKE78001 to running
game FAKE78005. The game, virtual pad and Remote Play were left untouched.
Wait for the console to be available before controlled native acceptance.

### 2026-10-27 — uninstall UI and startup title recreation

The native engine build was subsequently deployed and booted, followed by
8457593 (Xbox title/content uninstall). Both deployments used raw FTP transfer,
backup and exact read-back verification; no UID payload or Remote Play was used.
Native download execution and throughput are still not console-validated.

Gary revoked bulk Xbox deletion and will uninstall games himself. Only the old
imported Minishoot library entry and FAKE78002 title were removed before that
instruction; its source files remain. Do not resume bulk deletion. The virtual
pad was closed. Questions/status requests do not cancel ongoing authorized work.

The Xbox manifest is projected as external locations, so the standard installed
byte-count check incorrectly disabled Uninstall. Xbox manifest locations now
count as installed for that check and offer Uninstall rather than local-import
copy/remove actions. Confirmation preserves saves; the Xbox backend removes its
manifest entry and managed content after runtime title removal. A model regression
covers opening More and confirming Uninstall without any runtime title or Steam
installation. A separate jobs regression covers fresh completion versus reload.

Startup inventories existing Xbox downloads without automatically queuing PS5
title preparation. Only a successful native download completed in the current
process queues automatic preparation, once per request. Existing files can still
be prepared explicitly with Setup/Play. Legacy worker downloads now need explicit setup
on return. This avoids recreating titles the user removed from the home screen.
