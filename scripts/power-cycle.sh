#!/usr/bin/env bash
# Power-cycle all ports on the USB hub via uhubctl.
#
# Usage:
#   ./scripts/power-cycle.sh
set -euo pipefail

USB_HUB="1-9.3"

echo "Power-cycling all ports on hub $USB_HUB..."
sudo uhubctl -l "$USB_HUB" -a cycle
echo "Done. Waiting 3s for devices to re-enumerate..."
sleep 3
echo "Ready."
