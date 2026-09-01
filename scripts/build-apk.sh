#!/usr/bin/env bash
# Build a .papk file for a given Java example app.
#
# This is now a thin wrapper over Gradle's `:examples:<app>:assemblePapk`
# task. The plugin code lives in buildSrc/ (PicodroidPapkPlugin.kt).
#
# Usage:
#   ./scripts/build-apk.sh -a helloworld
#   ./scripts/build-apk.sh -a blinky -o /tmp/blinky.papk
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"
APP=""
OUTPUT=""
BOARD=""
STRIP_DEBUG=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -a, --app    <app>    Example app to build
  -o, --output <file>   Output path (default: build/apks/<app>.papk)
      --shrink          Apply the active release shrink map (class-name
                        shrinking). Off by default; also honored via
                        PICODROID_SHRINK=1. See docs/shrinker.md.
      --strip-debug     Drop LineNumberTable/SourceFile (and the rest of the
                        class-metadata strip) from the packed classes. For
                        device-bound PAPKs: that firmware never reads them.
                        Sim paths leave it off to keep (:line) stack traces.
  -b, --board  <name>   Target board: the API contract check also rejects
                        classes that board drops (framework_class_excludes
                        in its board.toml). Without it only the java/**
                        contract is checked.
  -h, --help            Show this help message

Apps:
$(list_apps "$REPO_ROOT/examples")
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)       usage; exit 0 ;;
    -a|--app)        APP="$2";    shift 2 ;;
    -o|--output)     OUTPUT="$2"; shift 2 ;;
    -b|--board)      BOARD="$2";  shift 2 ;;
    --shrink)        export PICODROID_SHRINK=1; shift ;;
    --strip-debug)   STRIP_DEBUG=1; shift ;;
    *)          echo "Unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$APP" ]]; then
  echo "Error: --app is required" >&2
  usage
  exit 1
fi

APP_DIR="$REPO_ROOT/examples/$APP"
if [[ ! -d "$APP_DIR" ]]; then
  echo "Error: app directory not found: $APP_DIR" >&2
  exit 1
fi

OUTPUT="${OUTPUT:-$REPO_ROOT/build/apks/${APP}.papk}"
mkdir -p "$(dirname "$OUTPUT")"

# Shrinking flag is passed as an explicit Gradle property, NOT left to the
# PICODROID_SHRINK env var alone: the plugin's env fallback reads
# System.getenv(), which inside a long-lived Gradle daemon is frozen to the
# environment the daemon was STARTED with — a daemon left behind by a
# --shrink run (e.g. the nightly's shrink-mode pass) silently shrink-stamps
# every later papk, which then fails `pdb install` against a no-shrink
# firmware with FrameworkVersionMismatch. A -P property is delivered
# per-invocation, so it cannot go stale.
#
# PICODROID_SKIP_GRADLE=1 skips the Gradle invocation and reuses an
# already-built papk. The Gradle :sim/:install tasks set this: they
# `dependsOn(assemblePapk)`, so the outer build already produced the papk —
# invoking ./gradlew again from inside a running build would deadlock on the
# project lock.
if [[ "${PICODROID_SKIP_GRADLE:-}" != "1" ]]; then
  # Same per-invocation-property rule for the network-test host (NET-7):
  # the env fallback exists for direct ./gradlew use, but a warm daemon's
  # environment is frozen, so this wrapper always forwards it as -P.
  GRADLE_EXTRA_ARGS=()
  if [[ -n "${PICODROID_NET_TEST_HOST:-}" ]]; then
    GRADLE_EXTRA_ARGS+=("-PpicodroidNetTestHost=${PICODROID_NET_TEST_HOST}")
  fi
  # The target board, when the caller knows it: verifyApiContract then also
  # rejects references to classes that board excludes from its framework.
  # A -P property for the same daemon-freshness reason as the shrink flag.
  if [[ -n "$BOARD" ]]; then
    GRADLE_EXTRA_ARGS+=("-Ppicodroid.board=${BOARD}")
  fi
  # Debug-attribute strip for device-bound PAPKs
  # (docs/designs/flash-string-budget-2026-08.md §4). Device firmware is built
  # with debug-assertions off (lib.sh build_firmware) and never reads
  # LineNumberTable/SourceFile; the sim does, which is why sim.sh never passes
  # this. A -P property for the same daemon-freshness reason as the shrink flag.
  if [[ "$STRIP_DEBUG" == "1" ]]; then
    GRADLE_EXTRA_ARGS+=("-Ppicodroid.stripDebug=true")
  fi
  # gradle_lock_run: pre-commit runs lanes in parallel and more than one can
  # reach Gradle (the typecheck stage, test.sh, sim-run.sh). Two gradlew
  # invocations against one project directory contend on Gradle's project lock,
  # and the papk they race over is what `pdb install` version-checks.
  (cd "$REPO_ROOT" && gradle_lock_run ./gradlew ":examples:$APP:assemblePapk" --console=plain \
    "-Ppicodroid.shrink=${PICODROID_SHRINK:-0}" \
    ${GRADLE_EXTRA_ARGS[@]+"${GRADLE_EXTRA_ARGS[@]}"})
fi

GRADLE_PAPK="$APP_DIR/build/papk/${APP}.papk"
if [[ ! -f "$GRADLE_PAPK" ]]; then
  echo "Error: expected Gradle output not found: $GRADLE_PAPK" >&2
  exit 1
fi
cp "$GRADLE_PAPK" "$OUTPUT"
size=$(stat -c%s "$OUTPUT" 2>/dev/null || stat -f%z "$OUTPUT")
echo "==> Wrote $OUTPUT ($size bytes)"
