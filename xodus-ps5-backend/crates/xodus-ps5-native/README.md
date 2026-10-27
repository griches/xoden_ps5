# Native PS5 downloader boundary — in development

This crate is the C interface being built for Playden's native Xbox downloader.
Its default build inspects a single 4096-byte XVD package header. The optional
`job-engine` build also exposes a worker lifecycle, using the shared authenticated
job implementation. Neither build has console runtime acceptance yet. A
successful header parse does not establish signature validity or ownership.

The exported ABI uses fixed-size fields, a version and a caller-provided output
size. Errors leave the caller's output unchanged. Header timestamps that overflow
the upstream nanosecond representation and narrowed enum values are rejected
before entering the upstream parser. No URLs, tokens, credentials or key material
are returned through this interface.

## Host verification

From the workspace root:

```
cargo test -p xodus-ps5-native --locked
cargo build -p xodus-ps5-native --release --locked
```

`tests/c_api.c` checks the C/Rust structure layout and invokes the exported
function with valid and invalid headers. On macOS it can be linked and run with:

```
cc -I crates/xodus-ps5-native/include crates/xodus-ps5-native/tests/c_api.c \
  target/release/libxodus_ps5_native.a -Wl,-dead_strip \
  -framework Security -framework CoreFoundation -framework SystemConfiguration \
  -liconv -lresolv -o /tmp/xodus-native-c-api
/tmp/xodus-native-c-api
```

## Isolated PS5 compile/link probe

Requires a nightly toolchain with `rust-src`, LLVM 18 and the PS5 payload SDK:

```
python3 crates/xodus-ps5-native/tools/build_probe.py \
  --sdk /absolute/path/to/ps5-payload-sdk \
  --llvm-config /absolute/path/to/llvm-config \
  --work-dir /absolute/path/to/isolated-build
```

This uses the workspace lockfile and never deploys or executes the output. It
copies Rust's standard-library source into the isolated build directory and
adapts a directory inode conversion for FreeBSD 11. The installed toolchain is
not edited. It builds with emulated TLS and the FreeBSD 11 libc layout. The work
directory records the compiler version and refuses reuse with another version.
Verified compiler: `rustc 1.101.0-nightly (16d1ffac1 2026-10-26)`.

The initial default FreeBSD target had a 224-byte `stat`, versus 120 bytes in the
PS5 SDK. The adapted target matches all 15 checked stat field offsets and total
size. This is **not** a complete platform ABI audit. The link probe supplies two
versioned stat aliases, a thread-name adapter, and an empty backtrace module
iterator. None of these have console runtime acceptance yet.

The optional full-engine probe (`--job-engine`) now links with the SDK's
`libSceRandom` through bounded `getentropy`/`getrandom` adapters. Adapter contract
and failure handling pass a host mock test; real console entropy, sockets,
threads and runtime execution still need acceptance. Directory-entry and event
layouts are asserted in both C and Rust before their versioned libc aliases are
linked. Keep using the existing worker until native console acceptance passes.

## Embedded engine checkpoint

The optional `job-engine` feature now embeds the existing CLI job modules as a
library, without CLI startup, GUI creation or signal handlers. The host supplies
the client, token manager and private roots. Subscription and entitlement checks
remain in the shared job runner. Typed progress callbacks omit paths, URLs and
credentials; phase callbacks follow successful durable job-result publication.
The default C interface remains header-only; the optional C job API is described below.

Verification: 73 shared-library tests pass. The CLI suite passes 125 tests with
one existing case-sensitive-path test failing on this Mac; that same test also
fails in a detached worktree at the previous commit (`c72ef04`). The optional
engine cross-compiles for the adapted PS5 target, but has not been linked into
Playden or executed on the console. A locally vendored, pinned XAL dependency
uses Rustls on FreeBSD to avoid requiring OpenSSL; other targets retain their
previous TLS selection. See `third_party/xal-rs/PS5-PATCH.md`.

The Rust `job` module now owns a dedicated runtime and an on-disk job lease.
Cancellation drops the operation, drains runtime blocking IO, records cancellation,
and only then releases the lease and exposes a terminal snapshot. It never deletes
partial files itself. A host regression test holds a simulated file operation open
and verifies the lease and unpublished cancellation remain until that operation
finishes. The runtime supports the existing NTFS parser's `block_in_place` bridge.
This lifecycle is now exposed through the optional C ABI, but is not yet wired into Playden.

## Optional C job API

`xodus_job_start/poll/cancel/release` use generation IDs, bounded snapshots and
caller-supplied output sizes. The host supplies approved absolute private storage
roots and its packaged PEM CA bundle. Certificate validation remains enabled;
empty/invalid bundles and attempts to switch trust anchors are rejected. Internal
account/catalogue clients share the configured trust. Existing credentials are
required; this interface does not launch sign-in UI or create an anonymous store.
The host must keep the library loaded and wait for a terminal snapshot before
cleanup or releasing a handle. Cancellation preserves partial files for the
host's managed cleanup policy. Completed means the job protocol finished, not
necessarily that a game is installed: package choice is a separate result.

Host verification: 74 shared engine tests and 5 native boundary/lifecycle tests
pass. The C lifecycle test verifies worker startup, setup failure, exclusive
handles, polling, release and stale-handle rejection. The full optional job
engine now compiles and links into `xodus-job-probe.elf` for the adapted PS5
target. This is a link result, not console download acceptance. The probe build
script never deploys or runs it.


## Native title DNS and execute-only code

The job-engine feature enables `xodus-cli/ps5-native-platform` on FreeBSD. Link
`tools/ps5_dns.c` with SceNet; the embedding title must already own network
initialization. DNS runs on the blocking pool with a five-second resolver timeout;
job cancellation drains it before releasing the lease. Output is four network-order
IPv4 octets; no hostname, token or signed URL is logged. Reqwest retains the URL
hostname for TLS verification and SNI. Desktop builds retain their normal resolver.
The mock `tests/dns.c` covers cleanup at each failure point and byte order.

PS5 native titles map text execute-only. AWS-LC X25519 assembly reads constants
from text and faults during TLS key generation. `build_probe.py` selects the
supported portable implementation with `AWS_LC_SYS_NO_ASM=1`, the cc builder,
and optimization level 2 for aws-lc-sys only. This keeps system toolchains and
normal desktop builds untouched. TLS/certificate checks are not disabled.

The native title must also implement `xodus_ps5_available_space(path, bytes)`.
Playden delegates to its existing validated volume query, which preserves its
console-space reserve and mount validation. Failure is an error, never unlimited
space. The link-only probe supplies a failing placeholder; it must not be used
as a downloader. `XODUS_NATIVE_TITLE` excludes that placeholder in Playden.
