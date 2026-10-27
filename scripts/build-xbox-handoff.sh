#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
out="$root/build/xbox-handoff"
mkdir -p "$out"
(cd "$root/tools/xbox-handoff" && "$root/scripts/with-env.sh" bash -c \
    'exec "$PS5_PAYLOAD_SDK/bin/prospero-clang" -Wall -Werror -O2 -ffile-prefix-map="$1"=. -o "$2" main.c -lSceUserService -lSceSystemService' \
    bash "$root" "$out/xbox-handoff.elf")
echo "Built Xbox handoff payload"
