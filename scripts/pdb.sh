#!/usr/bin/env bash
# Picodroid Debug Bridge — push apps to a device over UART.
#
# Usage: ./scripts/pdb.sh <command> [args...]
#
# Examples:
#   ./scripts/pdb.sh devices
#   ./scripts/pdb.sh -s /dev/cu.usbmodem1402 ping
#   ./scripts/pdb.sh -s /dev/cu.usbmodem1402 install build/apks/blinky.papk
#   ./scripts/pdb.sh -s /dev/cu.usbmodem1402 sysmon
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [[ $# -eq 0 || "$1" == "-h" || "$1" == "--help" ]]; then
  cat <<EOF
Usage: $(basename "$0") <command> [args...]

Commands:
  devices                         List available serial ports
  -s <port> ping                  Ping a connected device
  -s <port> install <file.papk>   Hot-swap an app onto the device
  -s <port> sysmon                Show system monitor stats (heap, tasks, CPU%)

Examples:
  ./scripts/pdb.sh devices
  ./scripts/pdb.sh -s /dev/cu.usbmodem1402 ping
  ./scripts/pdb.sh -s /dev/cu.usbmodem1402 install build/apks/blinky.papk
  ./scripts/pdb.sh -s /dev/cu.usbmodem1402 sysmon
EOF
  exit 0
fi

HOST_TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"

cargo run \
  --quiet \
  --target "$HOST_TARGET" \
  --manifest-path "$SCRIPT_DIR/../tools/pdb/Cargo.toml" \
  -- "$@"
