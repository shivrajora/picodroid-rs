#!/usr/bin/env bash
# Picodroid Debug Bridge — talk to a device over USB CDC, or to a running
# simulator over its socket.
#
# Thin launcher for the `pdb` binary (tools/pdb); all commands, flags and
# help live there, so this wrapper cannot drift out of date. Run with no
# arguments (or -h/--help) for the full usage.
#
# On a fleet bench (scripts/fleet-lib.sh) say which board with --board NAME
# or --slot NAME; the wrapper takes that board's lease and passes the serial
# of whichever board sits on the slot to the binary as -s. With one board
# configured, or when this session already holds one, the name can be left
# out.
#
# A simulator is not a bench board: `-e` or `-s sim` (the one running
# simulator), `-s emulator-<pid>` or `-s <socket>` (when several run) takes
# no lease and looks up no port.
#
# Examples:
#   ./scripts/pdb.sh devices
#   ./scripts/pdb.sh ping
#   ./scripts/pdb.sh --board pico_enviro_mon_w ping
#   ./scripts/pdb.sh install build/apks/blinky.papk
#   ./scripts/pdb.sh sysmon
#   ./scripts/pdb.sh input keyevent KEYCODE_DPAD_UP
#   ./scripts/pdb.sh devices -l
#   ./scripts/pdb.sh -s E6633861A37B9038 ping
#   ./scripts/pdb.sh -e install build/apks/helloworld.papk
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

# A simulator target: `-e`, `-s sim`, `-s emulator-<pid>`, or `-s <path>`
# naming a socket (or a socket file's name, for a simulator between its
# reboot's exec and its rebind).
sim_target=""
if [[ "${1:-}" == "-e" ]]; then
  sim_target=1
elif [[ "${1:-}" == "-s" ]]; then
  case "${2:-}" in
    sim|emulator-*|*.sock) sim_target=1 ;;
    *) if [[ -S "${2:-}" ]]; then sim_target=1; fi ;;
  esac
fi

# pdb mutates device state (install reboots the app, keyevents drive the UI
# another session may be measuring), so it shares the board lease with
# probe-rs. Help, host-side enumeration and a simulator do not need it.
case "${1:-}" in
  -h|--help|devices) ;;
  *) [[ -n "$sim_target" ]] || require_device_lock ${lock_args[@]+"${lock_args[@]}"} "$@" ;;
esac

# Fleet: name the board. Whichever board sits on the slot is looked up now
# from the slot's USB position — its serial, which pdb follows through a
# reboot, or its tty on older firmware; with several boards the binary must
# never auto-detect. `-d` ("the one USB device") means the slot's board.
if [[ -n "${PICODROID_BOARD_USB_PATH:-}" && "${1:-}" != "-s" && "${1:-}" != "-e" ]]; then
  [[ "${1:-}" != "-d" ]] || shift
  if ! target=$(usb_path_pdb_target "$PICODROID_BOARD_USB_PATH"); then
    echo "pdb: board of slot $PICODROID_SLOT is not enumerated (usb $PICODROID_BOARD_USB_PATH)" >&2
    exit 1
  fi
  set -- -s "$target" "$@"
fi

HOST_TARGET="$(host_target)"

cargo run \
  --quiet \
  --target "$HOST_TARGET" \
  --manifest-path "$SCRIPT_DIR/../tools/pdb/Cargo.toml" \
  -- "$@"
