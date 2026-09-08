#!/usr/bin/env bash
# Picodroid Debug Bridge — talk to a device over USB CDC.
#
# Thin launcher for the `pdb` binary (tools/pdb); all commands, flags and
# help live there, so this wrapper cannot drift out of date. Run with no
# arguments (or -h/--help) for the full usage.
#
# On a fleet bench (scripts/fleet-lib.sh) say which board with --board NAME
# or --slot NAME; the wrapper takes that board's lease and passes its current
# serial port to the binary as -s. With one board configured, or when this
# session already holds one, the name can be left out.
#
# Examples:
#   ./scripts/pdb.sh devices
#   ./scripts/pdb.sh ping
#   ./scripts/pdb.sh --board pico_enviro_mon_w ping
#   ./scripts/pdb.sh install build/apks/blinky.papk
#   ./scripts/pdb.sh sysmon
#   ./scripts/pdb.sh input keyevent KEYCODE_DPAD_UP
#   ./scripts/pdb.sh -s /dev/cu.usbmodem1402 ping
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

# No arguments: show the binary's help (and exit 0, as the wrapper always has).
if [[ $# -eq 0 ]]; then
  set -- --help
fi

# --board/--slot are for the lease, not for the binary: pull them out and
# keep everything else verbatim.
lock_args=()
args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --board|--slot) lock_args+=("$1" "${2:-}"); [[ $# -gt 1 ]] && shift ;;
    --board=*|--slot=*) lock_args+=("$1") ;;
    *) args+=("$1") ;;
  esac
  shift
done
set -- ${args[@]+"${args[@]}"}

# pdb mutates device state (install reboots the app, keyevents drive the UI
# another session may be measuring), so it shares the board lease with
# probe-rs. Help and host-side enumeration do not need it.
case "${1:-}" in
  -h|--help|devices) ;;
  *) require_device_lock ${lock_args[@]+"${lock_args[@]}"} "$@" ;;
esac

# Fleet: name the port. The pdb CDC device has no USB serial and its ttyACM
# number moves after a power cycle, so it is looked up now from the slot's
# USB position; with several boards the binary must never auto-detect.
if [[ -n "${PICODROID_BOARD_USB_PATH:-}" && "${1:-}" != "-s" ]]; then
  if ! port=$(usb_path_tty "$PICODROID_BOARD_USB_PATH"); then
    echo "pdb: board of slot $PICODROID_SLOT is not enumerated (usb $PICODROID_BOARD_USB_PATH)" >&2
    exit 1
  fi
  set -- -s "$port" "$@"
fi

HOST_TARGET="$(host_target)"

cargo run \
  --quiet \
  --target "$HOST_TARGET" \
  --manifest-path "$SCRIPT_DIR/../tools/pdb/Cargo.toml" \
  -- "$@"
