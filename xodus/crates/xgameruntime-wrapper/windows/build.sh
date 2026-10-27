#!/usr/bin/env bash
# Builds xgameruntime.dll (x86-64 Windows) inside the Lima VM:
#   limactl shell ps5 -- bash crates/xgameruntime-wrapper/windows/build.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
target=x86_64-pc-windows-gnu
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/xodus-target}"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
cd "$root"
libs=$(~/.cargo/bin/cargo rustc --release --target $target -p xgameruntime-wrapper --crate-type staticlib -- --print native-static-libs 2>&1 \
    | tee /dev/stderr | sed -n 's/.*native-static-libs: //p' | tail -1)
out="$root/target/wrapper"
mkdir -p "$out"
# -lwindows.0.xx.y import libraries ship inside the windows_x86_64_gnu crates.
search=()
for dir in "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/windows_x86_64_gnu-*/lib; do
    search+=(-L "$dir")
done
x86_64-w64-mingw32-gcc -shared -O2 -o "$out/xgameruntime.dll" \
    "$here/dllmain.c" "$here/xgameruntime.def" \
    "$CARGO_TARGET_DIR/$target/release/libxodus_wrapper.a" "${search[@]}" $libs \
    -lpthread -static -static-libgcc -Wl,--enable-stdcall-fixup
x86_64-w64-mingw32-strip "$out/xgameruntime.dll"
echo "==> $out/xgameruntime.dll"
