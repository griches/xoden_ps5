#!/usr/bin/env bash
# Package the selected direct Hui consumer locally, without console discovery, upload or launch.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
environment=${PLAYDEN_PS5_ENV:-"$root/scripts/with-env.sh"}
deps=${PLAYDEN_PS5_DEPS_DIR:-"$root/deps"}
hui=${PLAYDEN_HUI_SOURCE_DIR:-"$root/third_party/ps5-homebrew-ui"}
simulation=${PLAYDEN_UI_SIMULATION:-OFF}
native_access=${PLAYDEN_NATIVE_RUNTIME_ACCESS:-ON}
prospero_source=${PLAYDEN_PROSPERO_SOURCE_DIR:-"$root/third_party/prospero-win"}
helper_build="$root/third_party/PS5-Lapy-JB-Daemon/build/owned_root_helper-PPSA99324"
lapy_helper=${PLAYDEN_LAPY_HELPER_ELF:-"$helper_build/lapy.elf"}
lapy_manifest=${PLAYDEN_LAPY_HELPER_MANIFEST:-"$helper_build/lapy-manifest.json"}
title_remover=${PLAYDEN_TITLE_REMOVER_ELF:-"$root/build/title-remover/title-remover.elf"}
relaunch=${PLAYDEN_RELAUNCH_ELF:-"$root/build/self-update-relaunch/self-update-relaunch.elf"}
xbox_handoff="$root/build/xbox-handoff/xbox-handoff.elf"
case "$simulation" in
 ON|on|1|true) simulation=ON ;;
 OFF|off|0|false|'') simulation=OFF ;;
 *) echo "PLAYDEN_UI_SIMULATION must be ON or OFF" >&2; exit 2 ;;
esac
case "$native_access" in
 ON|on|1|true) native_access=ON ;;
 OFF|off|0|false) native_access=OFF ;;
 *) echo "PLAYDEN_NATIVE_RUNTIME_ACCESS must be ON or OFF" >&2; exit 2 ;;
esac
[[ -f "$environment" ]] || { echo "Missing explicit PS5 environment: $environment" >&2; exit 1; }
# PS5 CMake consumes the pre-baked atlas directory directly. Refresh it from the pinned submodule
# so catalog, font, license or baker changes cannot leave stale package inputs.
PLAYDEN_HUI_SOURCE_DIR="$hui" "$root/scripts/build-homebrew.sh" --prepare-fonts-only
if [[ $native_access == ON && -z ${PLAYDEN_TITLE_REMOVER_ELF:-} ]]; then
 "$root/scripts/build-title-remover.sh"
fi
if [[ $native_access == ON && -z ${PLAYDEN_RELAUNCH_ELF:-} ]]; then
 "$root/scripts/build-self-update-relaunch.sh"
fi
if [[ $native_access == ON ]]; then
 bash "$root/scripts/build-xbox-handoff.sh"
fi
if [[ ! -f "$root/scripts/network-deps/.deps/ps5/lib/libcurl.a" ]]; then
 "$root/scripts/network-deps/prepare-ps5-deps.sh"
fi
if [[ -n ${PLAYDEN_XBOX_WRAPPER_DLL:-} && -z ${PLAYDEN_XBOX_RUNTIME_DIR:-} ]]; then
 python3 "$root/tools/fetch_xbox_runtime.py"
fi
bash "$environment" bash -c 'exec "$PS5_PAYLOAD_SDK/bin/prospero-cmake" "$@"' bash \
 -G Ninja -S "$root" -B "$root/build/homebrew-ps5" -DCMAKE_BUILD_TYPE=Release \
 -DPLAYDEN_PLATFORM_PS5=ON -DPLAYDEN_PS5_TITLE_ID=PPSA99324 \
 -DPLAYDEN_UI_SIMULATION="$simulation" \
 -DPLAYDEN_NATIVE_RUNTIME_ACCESS="$native_access" -DPLAYDEN_PROSPERO_SOURCE_DIR="$prospero_source" \
 -DPLAYDEN_LAPY_HELPER_ELF="$lapy_helper" -DPLAYDEN_LAPY_HELPER_MANIFEST="$lapy_manifest" \
 -DPLAYDEN_TITLE_REMOVER_ELF="$title_remover" -DPLAYDEN_RELAUNCH_ELF="$relaunch" \
 -DPLAYDEN_XBOX_HANDOFF_ELF="$xbox_handoff" \
 -DPLAYDEN_XBOX_NATIVE_SOURCE="${PLAYDEN_XBOX_NATIVE_SOURCE:-}" \
 -DPLAYDEN_XBOX_NATIVE_LIBRARY="${PLAYDEN_XBOX_NATIVE_LIBRARY:-}" \
 -DPLAYDEN_XBOX_WRAPPER_DLL="${PLAYDEN_XBOX_WRAPPER_DLL:-}" \
 -DPLAYDEN_XBOX_RUNTIME_DIR="${PLAYDEN_XBOX_RUNTIME_DIR:-$root/deps/xbox-runtime}" \
 -DPLAYDEN_PS5_DEPS_DIR="$deps" -DPLAYDEN_HUI_SOURCE_DIR="$hui"
bash "$environment" cmake --build "$root/build/homebrew-ps5" --target playden-homebrew-ps5-package
echo "Selected Hui package (not deployed): $root/build/homebrew-ps5/dist/PPSA99324"
