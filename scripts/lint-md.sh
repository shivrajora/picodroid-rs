#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

if [[ ! -d "node_modules/markdownlint-cli2" ]]; then
  echo "ERROR: markdownlint-cli2 is not installed." >&2
  echo "       Run 'npm install' in the repo root, then re-run this script." >&2
  exit 1
fi

exec npx --no-install markdownlint-cli2 "$@"
