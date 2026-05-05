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
HEAP_LIMIT_KB="${PICODROID_HEAP_LIMIT_KB:-}"
EXTRA_ARGS=()
HOST_TARGET="$(host_target)"

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -b, --board <board>       Board to simulate (default: testbench_rp2350)
  -a, --app <app>           App to run (default: helloworld)
  -r, --release             Build in release mode
  -l, --heap-limit <KB>     Limit sim heap to KB kilobytes (simulates MCU)
      --shrink              Apply the active release class-name shrink map
                            (off by default; see docs/shrinker.md)
  -h, --help                Show this help message

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
    -l|--heap-limit)
      HEAP_LIMIT_KB="$2"
      shift 2
      ;;
    --shrink)
      export PICODROID_SHRINK=1
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
ENV_VARS=(PICODROID_APK_PATH="$APK_PATH")
if [[ -n "$HEAP_LIMIT_KB" ]]; then
  ENV_VARS+=(PICODROID_HEAP_LIMIT_KB="$HEAP_LIMIT_KB")
fi
if [[ "${PICODROID_SHRINK:-}" == "1" ]]; then
  ENV_VARS+=(PICODROID_SHRINK=1)
fi

env "${ENV_VARS[@]}" cargo run \
  -p picodroid \
  --target "$HOST_TARGET" \
  --no-default-features \
  --features "sim,$BOARD_FEATURE" \
  "${EXTRA_ARGS[@]}"
