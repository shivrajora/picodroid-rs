#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

BOARD="testbench_rp2350"
APP="blinky"
PROFILE="debug"
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -b, --board <board>  Target board (default: testbench_rp2350)
  -a, --app  <app>     App to build and flash (default: blinky)
  -r, --release        Build in release mode. Also drops the line-numbers
                       feature: debug builds print (File.java:39) stack-trace
                       frames, release builds print (pc=N) — resolve those
                       on the host with scripts/retrace.sh.
                       PICODROID_LINE_NUMBERS=0|1 overrides either way.
      --shrink         Apply the active release class-name shrink map
                       (off by default; see website/src/content/docs/reference/shrinker.md)
      --shrink-app     Also rename the app's own classes and private members
                       (requires --shrink; see build-apk.sh --shrink-app)
  -h, --help           Show this help message

Boards:
$(list_boards)

Apps:
$(list_apps "$SCRIPT_DIR/../examples")
EOF
      exit 0
      ;;
    -b|--board)
      BOARD="$2"
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
    --shrink)
      export PICODROID_SHRINK=1
      shift
      ;;
    --shrink-app)
      export PICODROID_SHRINK_APP=1
      shift
      ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

resolve_board "$BOARD"
# Before the build, so a busy board fails in a second rather than after
# minutes of cargo. The lease stays with this session until released.
require_device_lock --board "$BOARD" --app "$APP"
build_firmware

# Step 3: Flash the firmware (build is already up-to-date, so this just flashes).
# shellcheck disable=SC2086  # CARGO_PLUS is intentionally unquoted (empty or "+esp")
PICODROID_APK_PATH="$APK_PATH" cargo $CARGO_PLUS run \
  --manifest-path "$MANIFEST_DIR/Cargo.toml" \
  -p "$PACKAGE" \
  --jobs "$(cpu_count)" \
  --target "$TARGET" \
  --no-default-features \
  --features "$FIRMWARE_FEATURES" \
  "${EXTRA_BUILD_ARGS[@]}" \
  "${EXTRA_ARGS[@]}"
