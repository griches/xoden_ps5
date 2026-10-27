# Native PS5 downloader boundary — in development

This crate is the C interface being built for Playden's native Xbox downloader.
It currently inspects a single 4096-byte XVD package header. It does **not** sign
in, check entitlement, download, decrypt, extract, or launch a game. A successful
header parse does not establish signature validity or ownership.

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

`entropy_decl.h` only lets the TLS dependency compile. It does not implement
`getentropy`: a verified entropy adapter is mandatory before networking can be
linked/executed. Certificate trust configuration, socket/thread/runtime behaviour,
account refresh, package extraction, bounded progress, cooperative cancellation,
and native Playden job integration remain unfinished. Keep using the existing
worker until those requirements have been implemented and tested; do not claim
this probe is a working native downloader.
