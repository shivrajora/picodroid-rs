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
# Handle sanitizer defaults ON (docs/parity-audit.md HAL-05/X3): the 64-bit
# handle table silently absorbs use-after-delete lookups that dangle on real
# 32-bit hardware, so surfacing them loudly is the parity-honest default.
# Opt out with --no-sanitize-handles or PICODROID_HANDLE_SANITIZER=0.
SANITIZE_HANDLES="${PICODROID_HANDLE_SANITIZER:-1}"
EXTRA_ARGS=()
HOST_TARGET="$(host_target)"

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -b, --board <board>       Board to simulate (default: testbench_rp2350)
  -a, --app <app>           App to run (default: helloworld)
  -r, --release             Build in release mode
  -l, --heap-limit <KB>     Override the sim heap cap in KB. Defaults to the
                            simulated chip's FreeRTOS arena size (416 KB
                            RP2350 / 128 KB RP2040); pass 0 to disable the
                            cap entirely
  -S, --sanitize-handles    Abort with a backtrace on a use-after-delete LVGL
                            handle access — surfaces dangling-handle bugs the
                            sim otherwise hides. ON by default (parity-audit
                            HAL-05); kept as an explicit flag for clarity
      --no-sanitize-handles Disable the handle sanitizer (or set
                            PICODROID_HANDLE_SANITIZER=0)
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
  $(basename "$0") -b pico_enviro_mon -a picoenvmon --sanitize-handles
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
    -S|--sanitize-handles)
      SANITIZE_HANDLES=1
      shift
      ;;
    --no-sanitize-handles)
      SANITIZE_HANDLES=""
      shift
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

resolve_board "$BOARD"

# Step 1: Build the APK for the selected app.
bash "$SCRIPT_DIR/build-apk.sh" --app "$APP"

APK_PATH="$SCRIPT_DIR/../build/apks/${APP}.papk"

# Step 2: Compile and run the simulator with the APK embedded.
# Sim always targets the host — do not pass EXTRA_BUILD_ARGS (no -Zbuild-std for host).
ENV_VARS=(PICODROID_APK_PATH="$APK_PATH")
if [[ -n "$HEAP_LIMIT_KB" ]]; then
  ENV_VARS+=(PICODROID_HEAP_LIMIT_KB="$HEAP_LIMIT_KB")
fi
if [[ -n "$SANITIZE_HANDLES" ]]; then
  ENV_VARS+=(PICODROID_HANDLE_SANITIZER="$SANITIZE_HANDLES")
fi
if [[ "${PICODROID_SHRINK:-}" == "1" ]]; then
  ENV_VARS+=(PICODROID_SHRINK=1)
fi

# shellcheck disable=SC2086  # CARGO_PLUS is intentionally unquoted (empty or "+esp")
env "${ENV_VARS[@]}" cargo $CARGO_PLUS run \
  --manifest-path "$MANIFEST_DIR/Cargo.toml" \
  -p "$PACKAGE" \
  --target "$HOST_TARGET" \
  --no-default-features \
  --features "sim,$BOARD_FEATURE" \
  "${EXTRA_ARGS[@]}"
