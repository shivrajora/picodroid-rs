#!/usr/bin/env bash
# Build a .papk file for a given Java example app.
#
# This is now a thin wrapper over Gradle's `:examples:<app>:assemblePapk`
# task. The plugin code lives in buildSrc/ — see docs/writing-apps.md.
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
GRADLE_SHRINK_PROP=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -a, --app    <app>    Example app to build
  -o, --output <file>   Output path (default: build/apks/<app>.papk)
      --shrink          Apply the active release shrink map (class-name
                        shrinking). Off by default; also honored via
                        PICODROID_SHRINK=1. See docs/shrinker.md.
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
    --shrink)        export PICODROID_SHRINK=1; shift ;;
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

# Shrinking flag is propagated through PICODROID_SHRINK env var — the
# plugin reads it directly, so no -P needed.
(cd "$REPO_ROOT" && ./gradlew ":examples:$APP:assemblePapk" --console=plain)

GRADLE_PAPK="$APP_DIR/build/papk/${APP}.papk"
if [[ ! -f "$GRADLE_PAPK" ]]; then
  echo "Error: expected Gradle output not found: $GRADLE_PAPK" >&2
  exit 1
fi
cp "$GRADLE_PAPK" "$OUTPUT"
size=$(stat -c%s "$OUTPUT" 2>/dev/null || stat -f%z "$OUTPUT")
echo "==> Wrote $OUTPUT ($size bytes)"
