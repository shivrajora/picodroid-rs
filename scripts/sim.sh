#!/usr/bin/env bash
# Run a picodroid app in host simulator mode (no Pico hardware needed).
#
# Usage:
#   ./scripts/sim.sh                    # run default app (helloworld)
#   ./scripts/sim.sh --app blinky
#   ./scripts/sim.sh --app uart
#   ./scripts/sim.sh --app i2cdemo
#   ./scripts/sim.sh --release
set -e

APP="helloworld"
EXTRA_ARGS=()
HOST_TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --app <app>   App feature to run: helloworld (default), blinky, uart,
                arraydemo, inherit, interfacedemo, floatdemo, exceptiondemo,
                threaddemo, mathsdemo, i2cdemo, spidemo
  --release     Build in release mode
  -h, --help    Show this help message

Examples:
  $(basename "$0")
  $(basename "$0") --app blinky
  $(basename "$0") --app uart --release
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --app)
      APP="$2"
      shift 2
      ;;
    --release)
      EXTRA_ARGS+=("--release")
      shift
      ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

cargo run \
  --target "$HOST_TARGET" \
  --no-default-features \
  --features "sim,${APP}" \
  "${EXTRA_ARGS[@]}"
