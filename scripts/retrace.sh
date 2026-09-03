#!/usr/bin/env bash
# Make a device log readable on the host — picodroid's `retrace`, as ProGuard
# has one. Two things at once:
#   * the shrunk names a --shrink firmware prints go back to their originals;
#   * the `at Class.method(pc=N)` frames a release firmware prints (it carries
#     no LineNumberTable) become `at Class.method(File.java:LINE)`, resolved
#     from the unstripped class trees this checkout compiled.
#
# Usage: ./scripts/retrace.sh [<map.toml>] [--app <name>] < device.log > readable.log
#        ./scripts/sim.sh --app foo --shrink 2>&1 | ./scripts/retrace.sh
#        # an app-shrunk PAPK (--shrink-app) needs its own merged map:
#        ./scripts/sim.sh --app foo --shrink --shrink-app 2>&1 \
#          | ./scripts/retrace.sh build/apks/foo.shrink-map.toml
#        # a release device log, frames resolved for the SDK and the app:
#        ./scripts/retrace.sh --app foo < rtt.log
#
# Without a map argument the active map for this checkout's package version
# is used (`class-shrink print-version`, the same resolution the firmware and
# PAPK builds apply); with none active, only frames are resolved. The SDK
# tree (sdk/build/classes/java/main) is always consulted when it exists;
# --app adds that app's build/classes tree. The trees must come from the
# same sources the device runs — same checkout, same build — or a line can
# be off. See tools/class-shrink/src/retrace.rs.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

MAP=""
APP=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) APP="$2"; shift 2 ;;
    -h|--help) sed -n '2,23p' "$0"; exit 0 ;;
    *) MAP="$1"; shift ;;
  esac
done

HOST_TARGET="$(host_target)"
if [[ -z "$MAP" ]]; then
  version="$(cargo run -q -p class-shrink --target "$HOST_TARGET" -- print-version \
    --cargo-toml "$REPO_ROOT/platforms/rp/Cargo.toml" \
    --shrink-maps-dir "$REPO_ROOT/sdk/shrink-maps")"
  if [[ "$version" != "0.0.0" ]]; then
    MAP="$REPO_ROOT/sdk/shrink-maps/v${version}.toml"
  fi
fi

args=()
[[ -n "$MAP" ]] && args+=(--map "$MAP")
SDK_CLASSES="$REPO_ROOT/sdk/build/classes/java/main"
if [[ -d "$SDK_CLASSES" ]]; then
  args+=(--classes "$SDK_CLASSES")
else
  echo "retrace: $SDK_CLASSES is missing (run ./gradlew :sdk:compileJava) — SDK frames stay pc=N" >&2
fi
if [[ -n "$APP" ]]; then
  # Kotlin apps stage app + shim classes in one tree; Java apps compile
  # straight into classes/java/main. Both are unstripped, original names.
  for d in "$REPO_ROOT/examples/$APP/build/classes-staged" \
           "$REPO_ROOT/examples/$APP/build/classes/java/main"; do
    if [[ -d "$d" ]]; then
      args+=(--classes "$d")
      break
    fi
  done
  if [[ "${args[*]}" != *"examples/$APP/"* ]]; then
    echo "retrace: no compiled classes for $APP (run ./scripts/build-apk.sh --app $APP) — app frames stay pc=N" >&2
  fi
fi

if [[ ${#args[@]} -eq 0 ]]; then
  echo "retrace: no active shrink map and no class trees — nothing to retrace" >&2
  exec cat
fi
exec cargo run -q -p class-shrink --target "$HOST_TARGET" -- retrace "${args[@]}"
