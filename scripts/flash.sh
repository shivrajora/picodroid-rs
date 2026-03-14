#!/usr/bin/env bash
set -e

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

case "$CHIP" in
  rp2040)
    TARGET="thumbv6m-none-eabi"
    CHIP_FEATURE="chip-rp2040"
    ;;
  rp2350)
    TARGET="thumbv8m.main-none-eabihf"
    CHIP_FEATURE="chip-rp2350"
    ;;
  *)
    echo "Unknown chip: $CHIP. Use rp2040 or rp2350." >&2
    exit 1
    ;;
esac

APP_FEATURE="${APP:-blinky}"

cargo run \
  --target "$TARGET" \
  --no-default-features \
  --features "${APP_FEATURE},${CHIP_FEATURE}" \
  "${EXTRA_ARGS[@]}"
