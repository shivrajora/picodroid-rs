#!/usr/bin/env bash
# Map the shrunk names in a --shrink firmware's log back to their originals —
# picodroid's `retrace`, as ProGuard has one.
#
# Usage: ./scripts/retrace.sh [<map.toml>] < shrunk.log > readable.log
#        ./scripts/sim.sh --app foo --shrink 2>&1 | ./scripts/retrace.sh
#        # an app-shrunk PAPK (--shrink-app) needs its own merged map:
#        ./scripts/sim.sh --app foo --shrink --shrink-app 2>&1 \
#          | ./scripts/retrace.sh build/apks/foo.shrink-map.toml
#
# Without an argument the active map for this checkout's package version is
# used (`class-shrink print-version`, the same resolution the firmware and
# PAPK builds apply). Class tokens (`a/DK`, `a.DK`, `b/AK`, `b.AK`) and member
# targets in `.name(` position are substituted; everything else passes
# through. See tools/class-shrink/src/retrace.rs.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

HOST_TARGET="$(host_target)"
MAP="${1:-}"
if [[ -z "$MAP" ]]; then
  version="$(cargo run -q -p class-shrink --target "$HOST_TARGET" -- print-version \
    --cargo-toml "$REPO_ROOT/platforms/rp/Cargo.toml" \
    --shrink-maps-dir "$REPO_ROOT/sdk/shrink-maps")"
  if [[ "$version" == "0.0.0" ]]; then
    echo "retrace: no active shrink map for this package version — nothing to retrace" >&2
    exec cat
  fi
  MAP="$REPO_ROOT/sdk/shrink-maps/v${version}.toml"
fi

exec cargo run -q -p class-shrink --target "$HOST_TARGET" -- retrace --map "$MAP"
