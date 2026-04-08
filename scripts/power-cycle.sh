#!/usr/bin/env bash
# Power-cycle all ports on the USB hub that has the CMSIS-DAP debug probe.
#
# Usage:
#   ./scripts/power-cycle.sh
set -euo pipefail

# Auto-detect the USB hub location by finding the hub with a CMSIS-DAP probe.
USB_HUB=$(sudo uhubctl 2>/dev/null | awk '/^Current status for hub/{hub=$5} /CMSIS-DAP/{print hub}')

if [[ -z "$USB_HUB" ]]; then
  echo "ERROR: No USB hub with CMSIS-DAP probe detected." >&2
  exit 1
fi

echo "Power-cycling all ports on hub $USB_HUB..."
sudo uhubctl -l "$USB_HUB" -a cycle
echo "Done. Waiting 3s for devices to re-enumerate..."
sleep 3
echo "Ready."
