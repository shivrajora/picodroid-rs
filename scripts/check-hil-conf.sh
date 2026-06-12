#!/usr/bin/env bash
# Drift guard for hil-tests.conf: catches test rows that silently rot when
# example apps are renamed, removed, or coalesced.
#
# Checks, per non-comment row (app|category|timeout|patterns[|pdb_command]):
#   1. examples/<app>/ exists
#   2. category is one of term|loop|hw|pdb|skip, timeout is numeric
#   3. every expected pattern of the form `Tag[]:] ...` has its tag string
#      present as a Java string literal ("Tag") somewhere under examples/ or
#      sdk/ — so deleting/renaming the demo that emits the tag fails here at
#      commit time instead of turning the nightly suites permanently red
#      (which is exactly what happened when 516cf79 coalesced the langsuite
#      sub-demos but the conf kept their old tags).
#
# Tags built dynamically (string concatenation) can be exempted via
# ALLOW_TAGS below.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# Optional argument overrides the conf path (used by the negative self-test).
CONF="${1:-$SCRIPT_DIR/hil-tests.conf}"

# Tags that don't appear verbatim as a Java string literal (e.g. computed at
# runtime). Keep this list short and justified.
ALLOW_TAGS=()

errors=0
fail() {
  echo "ERROR: $*" >&2
  errors=$((errors + 1))
}

allow_tag() {
  local tag="$1" a
  for a in ${ALLOW_TAGS[@]+"${ALLOW_TAGS[@]}"}; do
    [[ "$a" == "$tag" ]] && return 0
  done
  return 1
}

lineno=0
while IFS='|' read -r app category timeout patterns pdb_command; do
  lineno=$((lineno + 1))
  [[ "$app" =~ ^[[:space:]]*# ]] && continue
  [[ -z "$app" ]] && continue

  if [[ ! -d "$REPO_ROOT/examples/$app" ]]; then
    fail "line $lineno: app '$app' has no examples/$app/ directory"
  fi

  case "$category" in
    term|loop|hw|pdb|skip) ;;
    *) fail "line $lineno ($app): unknown category '$category'" ;;
  esac

  if [[ ! "$timeout" =~ ^[0-9]+$ ]]; then
    fail "line $lineno ($app): timeout '$timeout' is not numeric"
  fi

  if [[ "$category" == "pdb" && -z "${pdb_command:-}" ]]; then
    fail "line $lineno ($app): pdb row is missing its pdb_command field"
  fi

  # Tag-literal check. Only patterns shaped `Tag[]:] ...` carry a tag; free
  #-form regex patterns (e.g. `TOTAL: .* ms`) are skipped.
  IFS=';' read -ra pats <<< "$patterns"
  for pat in ${pats[@]+"${pats[@]}"}; do
    [[ -z "$pat" ]] && continue
    tag="$(sed -n 's/^\([A-Za-z][A-Za-z0-9]*\)\[\]:\].*/\1/p' <<< "$pat")"
    [[ -z "$tag" ]] && continue
    allow_tag "$tag" && continue
    if ! grep -rqF "\"$tag\"" "$REPO_ROOT/examples" "$REPO_ROOT/sdk" --include='*.java'; then
      fail "line $lineno ($app): pattern tag '$tag' not found as a string literal in examples/ or sdk/ Java sources (renamed or deleted demo? update the conf row or ALLOW_TAGS)"
    fi
  done
done < "$CONF"

if [[ $errors -gt 0 ]]; then
  echo "" >&2
  echo "check-hil-conf: $errors error(s) — hil-tests.conf is out of sync with the source tree." >&2
  exit 1
fi

echo "check-hil-conf: OK"
