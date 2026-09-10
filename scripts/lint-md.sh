#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
BIN="$REPO_ROOT/scripts/lint-md/node_modules/.bin/markdownlint-cli2"

# The linter's npm project lives in scripts/lint-md/, but the lint always runs
# from the repo root: markdownlint-cli2 discovers .markdownlint-cli2.jsonc in
# the current directory and its `globs` are resolved against that directory.
cd "$REPO_ROOT"

if [[ ! -x "$BIN" ]]; then
  echo "ERROR: markdownlint-cli2 is not installed." >&2
  echo "       Run '(cd scripts/lint-md && npm ci)', then re-run this script." >&2
  exit 1
fi

exec "$BIN" "$@"
