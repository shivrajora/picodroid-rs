#!/usr/bin/env bash
# Power-cycle the Pico 2 and/or Debug Probe via uhubctl.
#
# Usage:
#   ./scripts/power-cycle.sh          # cycle both devices
#   ./scripts/power-cycle.sh --probe  # cycle debug probe only
#   ./scripts/power-cycle.sh --pico   # cycle pico only
set -euo pipefail

USB_HUB="1-9.3"
USB_PORT_PROBE=3
USB_PORT_PICO=2

CYCLE_PROBE=false
CYCLE_PICO=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --probe) CYCLE_PROBE=true; shift ;;
    --pico)  CYCLE_PICO=true; shift ;;
    -h|--help)
      cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Power-cycle the Pico 2 and/or Debug Probe on USB hub $USB_HUB.

Options:
  --probe   Cycle the debug probe only (port $USB_PORT_PROBE)
  --pico    Cycle the Pico 2 only (port $USB_PORT_PICO)
  -h, --help  Show this help message

With no options, both devices are cycled.
EOF
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

# Default: cycle both.
if [[ "$CYCLE_PROBE" == "false" && "$CYCLE_PICO" == "false" ]]; then
  CYCLE_PROBE=true
  CYCLE_PICO=true
fi

PORTS=""
if [[ "$CYCLE_PICO" == "true" ]]; then
  PORTS="$USB_PORT_PICO"
fi
if [[ "$CYCLE_PROBE" == "true" ]]; then
  if [[ -n "$PORTS" ]]; then
    PORTS="$PORTS,$USB_PORT_PROBE"
  else
    PORTS="$USB_PORT_PROBE"
  fi
fi

echo "Power-cycling port(s) $PORTS on hub $USB_HUB..."
sudo uhubctl -l "$USB_HUB" -p "$PORTS" -a cycle
echo "Done. Waiting 3s for devices to re-enumerate..."
sleep 3
echo "Ready."
