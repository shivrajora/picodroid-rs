#!/usr/bin/env bash
# Picodroid Debug Bridge — talk to a device over USB CDC.
#
# Thin launcher for the `pdb` binary (tools/pdb); all commands, flags and
# help live there, so this wrapper cannot drift out of date. Run with no
# arguments (or -h/--help) for the full usage.
#
# Examples:
#   ./scripts/pdb.sh devices
#   ./scripts/pdb.sh ping
#   ./scripts/pdb.sh install build/apks/blinky.papk
#   ./scripts/pdb.sh sysmon
#   ./scripts/pdb.sh input keyevent KEYCODE_DPAD_UP
#   ./scripts/pdb.sh -s /dev/cu.usbmodem1402 ping
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

# No arguments: show the binary's help (and exit 0, as the wrapper always has).
if [[ $# -eq 0 ]]; then
  set -- --help
fi

# pdb mutates device state (install reboots the app, keyevents drive the UI
# another session may be measuring), so it shares the board lease with
# probe-rs. Help and host-side enumeration do not need it.
case "$1" in
  -h|--help|devices) ;;
  *) require_device_lock "$@" ;;
esac

HOST_TARGET="$(host_target)"

cargo run \
  --quiet \
  --target "$HOST_TARGET" \
  --manifest-path "$SCRIPT_DIR/../tools/pdb/Cargo.toml" \
  -- "$@"
