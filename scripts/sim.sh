#!/usr/bin/env bash
# Run a picodroid app in host simulator mode (no Pico hardware needed).
#
# Usage:
#   ./scripts/sim.sh                    # run default app (helloworld)
#   ./scripts/sim.sh --app blinky
#   ./scripts/sim.sh --board pico_enviro_mon --app helloworld
#   ./scripts/sim.sh --release
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

BOARD="testbench_rp2350"
APP="helloworld"
EXTRA_ARGS=()
HOST_TARGET="$(host_target)"

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -b, --board <board>  Board to simulate (default: testbench_rp2350)
  -a, --app <app>      App to run (default: helloworld)
  -r, --release        Build in release mode
  -h, --help           Show this help message

Boards:
$(list_boards)

Apps:
$(list_apps "$SCRIPT_DIR/../examples")

Examples:
  $(basename "$0")
  $(basename "$0") -a blinky
  $(basename "$0") -b pico_enviro_mon -a helloworld
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
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
      EXTRA_ARGS+=("--release")
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

# Resolve board feature name (underscores → hyphens)
BOARD_FEATURE="board-$(echo "$BOARD" | tr '_' '-')"

# Step 1: Build the APK for the selected app.
bash "$SCRIPT_DIR/build-apk.sh" --app "$APP"

APK_PATH="$SCRIPT_DIR/../build/apks/${APP}.papk"

# Step 2: Compile and run the simulator with the APK embedded.
PICODROID_APK_PATH="$APK_PATH" cargo run \
  --target "$HOST_TARGET" \
  --no-default-features \
  --features "sim,$BOARD_FEATURE" \
  "${EXTRA_ARGS[@]}"
