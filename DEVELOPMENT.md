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
probe for the embedded downloader. For the final PS5 app build set both:

```text
PLAYDEN_XBOX_NATIVE_SOURCE=<checkout>/xodus-ps5-backend/crates/xodus-ps5-native
PLAYDEN_XBOX_NATIVE_LIBRARY=<native-build>/target/x86_64-unknown-freebsd/release/libxodus_ps5_native.a
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
