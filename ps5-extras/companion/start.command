#!/bin/zsh
set -eu
cd -- "$(dirname -- "$0")"
exec node --disable-warning=ExperimentalWarning server.mjs
