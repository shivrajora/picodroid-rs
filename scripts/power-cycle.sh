#!/usr/bin/env bash
# Power-cycle the bench. With a fleet config (scripts/fleet-lib.sh) that is
# the probe port and the board port of ONE slot -- the other boards keep
# running; without one it is every port of the hub that carries the
# CMSIS-DAP probe.
#
# Usage:
#   ./scripts/power-cycle.sh                      # the slot you hold, or the only one
#   ./scripts/power-cycle.sh --board pico_enviro_mon_w
#   ./scripts/power-cycle.sh --slot testbench_rp2350
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

require_device_lock "$@"

echo "Power-cycling${PICODROID_SLOT:+ slot $PICODROID_SLOT}..."
power_cycle_bench
echo "Done. Waiting 3s for devices to re-enumerate..."
sleep 3
echo "Ready."
