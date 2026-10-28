# prospero-win runtime kits

A kit is the Windows runtime Playden installs on the console: Wine and its PS5 libraries, DXVK, the template bottle every
game's bottle is copied from, the PS5 title shell and the shipped recipes. Kits are built outside this repository and
installed separately from the app; app builds never build Wine or download a kit.
At first start, XoDen offers to download its matching kit from the separate public
[XoDen runtime mirror](https://github.com/griches/xoden-runtime/releases/tag/v0.13.1),
using its `playden-update.json` and the compiled manifest hash. Kit 0.13.1 adds a title-specific runtime lookup fallback to the native shell; Wine, Mesa, the helper and bottle template remain unchanged. The exact shell source, patch and provenance accompany the release.
The kit release endpoint is independent of XoDen's app version. Upstream Playden
continues to use its own release discovery when no mirror is configured. It can also be uploaded manually.

## What Playden pins

- `third_party/prospero-win`: the prospero-win source the kit was packaged from. `dependencies.lock.json`
  (`prospero_win.revision`) records the exact commit, and builds refuse another revision or a dirty checkout.
  Initialize it with `git submodule update --init third_party/prospero-win`.
- `src/runtime/kit_trust.h`: the kit this build was tested with (its id and manifest SHA-256), shown next to the
  installed kit in Settings › About. `dependencies.lock.json` (`prospero_kit`) records the same kit with its runtime
  revision and how it was built; a host test checks the two agree.

Playden uses the kit installed at `/data/playden/kits/<kit id>` (`playden-kit` since kit 0.13) as long as it is
complete and its files match its own manifest; an incomplete or changed kit is refused at start-up.

## Building and packaging

The scripts that built the current kit are in [`tools/kit-h`](../../tools/kit-h/README.md). They build Wine and the
PS5 libraries on a Linux machine, stage the inputs and write a kit spec. The kit is then packaged with prospero-win's
packager:

```sh
python3 third_party/prospero-win/tools/package_playden_kit.py package --spec /absolute/path/kit-spec.json \
    --out /absolute/path/kit
```

Kits with host build paths in their runtime files (development kits) need `--development-build-paths`.
`tools/console_kit_upload.py` uploads a packaged kit to the console.

Packaging checks every input against the spec and the source pins; a pinned source does not by itself certify the
runtime. `prospero_kit_producer_consumer` checks that a manifest written by the packager is accepted by Playden's
verifier and that changed or extra files are refused.
