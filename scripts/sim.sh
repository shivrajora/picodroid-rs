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
MEM_DIAG=""
EXTRA_ARGS=()
SIM_ARCH="host"

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
      --arch <host|arm32>   Architecture to build and run the sim for
                            (default: host). 'arm32' cross-compiles to
                            32-bit ARM Linux and runs under qemu-user, so the
                            sim gets the device's pointer width and
                            instruction set. Runs headless by default. NOTE:
                            at 32 bits the default handle path is the
                            device's raw-pointer cast, where the handle
                            sanitizer has nothing to check — that IS the
                            device behavior being modeled. Build with
                            handle-table-32 for the generational table.
                            Needs qemu-user-static + gcc-arm-linux-gnueabihf
  -m, --mem-diag            Compile in the memory diagnostics (mem-diag
                            feature): periodic [memmon] heap monitor +
                            steady-state growth sentinel (warn-only unless
                            PICODROID_MEMDIAG_STRICT=1). Tunables via
                            PICODROID_MEMDIAG_WINDOW_MS / _SENTINEL / _STRICT
                            / _OFFENSIVE / _HISTO; on-demand snapshot via
                            'sim-ctrl.sh memstats'. See
                            docs/memory-diagnostics.md
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
    --arch)
      SIM_ARCH="$2"
      shift 2
      ;;
    -m|--mem-diag)
      MEM_DIAG=1
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

SIM_TARGET="$(sim_target "$SIM_ARCH")" || exit 1
if [[ "$SIM_ARCH" != "host" ]]; then
  if ! have_arm32_sim; then
    echo "--arch $SIM_ARCH requested but the toolchain is incomplete." >&2
    arm32_sim_hint
    exit 1
  fi
  # Headless by default: there are no armhf X11 libraries in the cross
  # sysroot, so a windowed run would only fall back to headless anyway —
  # this way the logs say so up front instead of after a failed open.
  export PICODROID_SIM_HEADLESS="${PICODROID_SIM_HEADLESS:-1}"
fi

resolve_board "$BOARD"

# Step 1: Build the APK for the selected app.
bash "$SCRIPT_DIR/build-apk.sh" --app "$APP"

APK_PATH="$SCRIPT_DIR/../build/apks/${APP}.papk"

# Step 2: Compile and run the simulator with the APK embedded.
# The sim targets the host, or armv7 Linux under qemu with --arch arm32 — either
# way a hosted target, so do not pass EXTRA_BUILD_ARGS (no -Zbuild-std here).
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

FEATURES="sim,$BOARD_FEATURE"
if [[ -n "$MEM_DIAG" ]]; then
  FEATURES="$FEATURES,mem-diag"
  # Sensible defaults when diagnostics are compiled in: monitor is always
  # active; the growth sentinel defaults ON in warn-only mode (override any
  # of these by exporting the variable yourself; strict/offensive/histo stay
  # opt-in). See docs/memory-diagnostics.md.
  ENV_VARS+=(PICODROID_MEMDIAG_SENTINEL="${PICODROID_MEMDIAG_SENTINEL:-1}")
fi

# shellcheck disable=SC2086  # CARGO_PLUS is intentionally unquoted (empty or "+esp")
env "${ENV_VARS[@]}" cargo $CARGO_PLUS run \
  --manifest-path "$MANIFEST_DIR/Cargo.toml" \
  -p "$PACKAGE" \
  --target "$SIM_TARGET" \
  --no-default-features \
  --features "$FEATURES" \
  "${EXTRA_ARGS[@]}"
