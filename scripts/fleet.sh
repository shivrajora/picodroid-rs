#!/usr/bin/env bash
# Bench fleet operator commands (see scripts/fleet-lib.sh for the config).
#
#   ./scripts/fleet.sh discover            probes and boards on the host, with
#                                          sysfs positions and hub/port
#   ./scripts/fleet.sh check               validate the fleet config
#   ./scripts/fleet.sh slots               list the configured slots
#   ./scripts/fleet.sh status              lease status of every slot
#   ./scripts/fleet.sh tty SLOT            the board's current /dev/ttyACM
#   ./scripts/fleet.sh cycle SLOT|--board B   power-cycle one slot (takes its lease)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

cmd="${1:-}"; [[ $# -gt 0 ]] && shift
case "$cmd" in
  discover) fleet_discover ;;
  check)    fleet_check_conf && echo "fleet: OK ($(fleet_conf_path))" ;;
  slots)    fleet_enabled || { echo "fleet: no config" >&2; exit 1; }; fleet_list_slots ;;
  status)   bash "$SCRIPT_DIR/device-lock.sh" status ;;
  tty)
    [[ -n "${1:-}" ]] || { echo "usage: fleet.sh tty SLOT" >&2; exit 1; }
    path=$(fleet_slot_field "$1" board_usb_path) || { echo "fleet: unknown slot '$1'" >&2; exit 1; }
    usb_path_tty "$path" || { echo "fleet: board of slot $1 is not enumerated (usb $path)" >&2; exit 1; }
    ;;
  cycle)
    fleet_enabled || { echo "fleet: no config" >&2; exit 1; }
    if [[ "${1:-}" == --board || "${1:-}" == --slot ]]; then
      require_device_lock "$@"
    else
      [[ -n "${1:-}" ]] || { echo "usage: fleet.sh cycle SLOT|--board B" >&2; exit 1; }
      require_device_lock --slot "$1"
    fi
    power_cycle_slot "$PICODROID_SLOT"
    ;;
  -h|--help|help|"") sed -n '2,10p' "$0" ;;
  *) echo "fleet.sh: unknown command '$cmd'" >&2; sed -n '2,10p' "$0" >&2; exit 1 ;;
esac
