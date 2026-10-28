# Development and provenance

This repository brings the existing app, backend and integration-tool repositories
under one private GitHub remote. Their original commit histories are retained as
Git subtrees. Component heads at import are recorded in
[SOURCE-PROVENANCE.json](SOURCE-PROVENANCE.json). Original working repositories
and their upstream remotes were left unchanged.

The `xodus/` subtree also includes the earlier local CLI changes and the runtime
wrapper source that had not been committed in its original checkout. This is
historical source; `xodus-ps5-backend/` is the current native downloader backend.

## Dependencies and licenses

Run `git submodule update --init --recursive` from this repository's root. The
root `.gitmodules` maps Playden's original submodule paths under `playden-ps5/`
and pins Xodus Nexus at its existing revision. No runtime-kit or SDK pins were
changed for this import. Nested upstream `.gitmodules` files are retained as
source history; Git uses the root mapping in this combined checkout.

Component licenses and notices remain in their directories: Playden and Xodus
are GPL-3.0; Xodus Nexus is MIT. Third-party submodules and vendored libraries
retain their own notices. This repository does not contain game downloads,
account state, native SDK build folders or deployed runtime-kit binaries.
Legacy integration documentation describes earlier prototypes; use the latest
[work record](xodus-games/GAMEPASS-PS5-RECORD.md) for accepted current behavior.

## Building the app

Read `playden-ps5/CLAUDE.md` and `playden-ps5/docs/building.md` first. Run its build
commands from `playden-ps5/`. The native backend build instructions live at
`xodus-ps5-backend/crates/xodus-ps5-native/README.md`; use its `--job-engine`
probe for the embedded downloader. For the final PS5 app build set:

```text
PLAYDEN_XBOX_NATIVE_SOURCE=<checkout>/xodus-ps5-backend/crates/xodus-ps5-native
PLAYDEN_XBOX_NATIVE_LIBRARY=<native-build>/target/x86_64-unknown-freebsd/release/libxodus_ps5_native.a
PLAYDEN_XBOX_WRAPPER_DLL=<wrapper-build>/xgameruntime.dll
```

Fetch pinned dependencies using the app's existing scripts. Do not copy local
account files or ignored companion configuration into Git.

## Keeping existing working checkouts in sync

During the transition, code and console build outputs may still live in the
original sibling checkouts. Commit the component change there, then bring it into
this repository from its root (without squashing) before pushing:

```sh
git subtree pull --prefix=playden-ps5 ../playden-ps5 xbox-store
git subtree pull --prefix=xodus-ps5-backend ../xodus-ps5-backend ps5-account-backend
git subtree pull --prefix=xodus-games ../xodus-games main
git push origin main
```

Alternatively develop directly in the component directories here. Do not push
this combined `main` branch to any upstream project. Imported upstream release
workflows remain inside component directories; no release workflow is installed
at this repository's root. GitHub Actions is disabled on the private repository
at initial publication.

## Compatibility reports

The top-level [COMPATIBILITY.md](COMPATIBILITY.md) is the maintained PC Game Pass
list. The issue form is `.github/ISSUE_TEMPLATE/compatibility-report.yml`, which
creates reports only in this repository. The imported upstream Steam list remains
in `playden-ps5/COMPATIBILITY.md` for reference and is not our Game Pass verdict.
App ratings are still local; no automatic GitHub reporting is enabled.

Updating documentation does not require a console session. Runtime changes need
the component's required checks; host tests alone do not establish PS5 behavior.
Do not take controller control or interrupt a game for a repository update.

## XoDen package identity and runtime mirror

The XoDen app now builds as `PPSA99324`, with content ID
`UP9000-PPSA99324_00-XODEN0000000000`. The package includes a newly built,
exact-title permission helper; its manifest protocol is unchanged and its digest
is recorded under that title in `playden-ps5/dependencies.lock.json`.
The Xbox handoff payload returns to the new title, and the in-game Xbox wrapper
loads certificates from the new package path. Existing shared data and kit paths
under `/data/playden` are retained.

The runtime download lives in the public, separate
[`griches/xoden-runtime`](https://github.com/griches/xoden-runtime) repository,
as GitHub release assets rather than Git blobs. Original release `v0.13.0` mirrors
Playden `v0.7.2` kit 0.13 byte-for-byte. Current `v0.13.1` rebuilds only the native
launcher with a title-specific runtime source fallback, and updates its build
marker, kit manifest and discovery metadata. Wine, graphics libraries, helper and
bottle template are unchanged. Exact shell source, patch and Git bundle are included.
Provenance, source revisions, notices and SHA-256 checksums accompany it. The app
uses the pinned release API endpoint and still checks its compiled kit manifest
hash, the archive digest and unpacked file hashes. App updates remain disabled
for the Game Pass mode.

For future kit changes, create a separate runtime release, verify all inputs,
test the kit, then change its configured release URL and compiled trust pin.
Do not repoint a published version to different bytes.

The Xbox Wine implementation DLLs are also packaged with the app under
`xbox-runtime/`, alongside XoDen's wrapper. The builder fetches the pinned
Xodus Nexus runtime archive when these dependencies are absent and verifies
both the archive and extracted module hashes. This removes the earlier manual
console setup dependency at `/data/playden/xbox/runtime`. Source and license
information travel with the package; see `runtime/xbox/` and the `xbox_runtime`
entry in `dependencies.lock.json`.

## 0.1.0 release

XoDen uses its own app version 0.1.0 and tag `0.1`; kit 0.13.1 keeps its
independent version. Build the native job engine and the Xbox wrapper from the
active backend before packaging the app. The PS5 packaging script fetches the
checksum-pinned Xbox modules and Microsoft shader compiler when needed. Their
licences/notices are copied into the app, not relicensed as GPL.

Release checks: full app host suite, Rust CLI library tests, native cross-build,
PS5 package build, sign-in helper build/tests, archive contents/checksums and
credential/private-configuration review. Run helper checks from
`xodus-games/ps5-extras/signin`: `npm ci --ignore-scripts`, `npm run build`,
then `npm test`. Node.js 24+ is required.

The app archive contains the PPSA99324 folder, optional Python uploader,
installation notes and notices. The separate helper archive includes its built
browser adapter, relay/server, launcher, source and licence. It needs Node and
Chrome but does not need npm installation for normal use. GitHub source archives
do not contain submodule contents; use a recursive clone to build from source.

Release preparation does not deploy to the PS5 or interrupt active downloads.
Keep console backups, output folders and private diagnostics outside the commit
and release assets. Automatic app updating remains disabled for Game Pass mode.
