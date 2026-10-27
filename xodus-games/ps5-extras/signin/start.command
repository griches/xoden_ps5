#!/bin/zsh
set -eu
cd -- "${0:A:h}"
if ! command -v node >/dev/null; then
  print 'Install Node.js, then reopen this sign-in helper.'
  read -r '?Press Return to close.'
  exit 1
fi
if [[ ! -f build/browser.mjs ]]; then
  npm ci --ignore-scripts --no-audit --no-fund
  npm run build
fi
exec node server.mjs --open
