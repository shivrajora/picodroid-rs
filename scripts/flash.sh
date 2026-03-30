#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

CHIP="rp2040"
APP="blinky"
PROFILE="debug"
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -c, --chip <chip>   Target chip: rp2040 (default) or rp2350
  -a, --app  <app>    App to build and flash (default: blinky)
  -h, --help          Show this help message

Apps:
$(list_apps "$SCRIPT_DIR/../examples")
EOF
      exit 0
      ;;
    -c|--chip)
      CHIP="$2"
      shift 2
      ;;
    -a|--app)
      APP="$2"
      shift 2
      ;;
    -r|--release)
      PROFILE="release"
      EXTRA_ARGS+=("$1")
      shift
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

# Step 2: Build the firmware, embedding the APK.
JOBS=$(cpu_count)
PICODROID_APK_PATH="$APK_PATH" cargo build \
  --jobs "$JOBS" \
  --target "$TARGET" \
  --no-default-features \
  --features "$CHIP_FEATURE" \
  "${EXTRA_ARGS[@]}"

ELF="target/${TARGET}/${PROFILE}/picodroid"

if [[ ! -f "$ELF" ]]; then
  echo "Binary not found: $ELF" >&2
  exit 1
fi

print_memory_usage "$ELF"

# Step 3: Flash the firmware (build is already up-to-date, so this just flashes).
PICODROID_APK_PATH="$APK_PATH" cargo run \
  --jobs "$JOBS" \
  --target "$TARGET" \
  --no-default-features \
  --features "$CHIP_FEATURE" \
  "${EXTRA_ARGS[@]}"
