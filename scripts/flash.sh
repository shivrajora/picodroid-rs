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
  -r, --release        Build in release mode
      --shrink         Apply the active release class-name shrink map
                       (off by default; see docs/shrinker.md)
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
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

resolve_board "$BOARD"
build_firmware

# Step 3: Flash the firmware (build is already up-to-date, so this just flashes).
PICODROID_APK_PATH="$APK_PATH" cargo run \
  -p picodroid \
  --jobs "$(cpu_count)" \
  --target "$TARGET" \
  --no-default-features \
  --features "$BOARD_FEATURE" \
  "${EXTRA_ARGS[@]}"
