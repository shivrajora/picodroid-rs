#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

CHIP="rp2040"
APP=""
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

APP_FEATURE="${APP:-blinky}"
JOBS=$(cpu_count)

cargo run \
  --jobs "$JOBS" \
  --target "$TARGET" \
  --no-default-features \
  --features "${APP_FEATURE},${CHIP_FEATURE}" \
  "${EXTRA_ARGS[@]}"
