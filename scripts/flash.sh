#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

CHIP="rp2040"
APP="blinky"
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --chip)
      CHIP="$2"
      shift 2
      ;;
    --app)
      APP="$2"
      shift 2
      ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

resolve_chip "$CHIP"

# Step 1: Build the APK for the selected app.
bash "$SCRIPT_DIR/build-apk.sh" --app "$APP"

APK_PATH="$SCRIPT_DIR/../build/apks/${APP}.papk"

# Step 2: Build and flash the firmware with the APK embedded.
JOBS=$(cpu_count)
PICODROID_APK_PATH="$APK_PATH" cargo run \
  --jobs "$JOBS" \
  --target "$TARGET" \
  --no-default-features \
  --features "$CHIP_FEATURE" \
  "${EXTRA_ARGS[@]}"
