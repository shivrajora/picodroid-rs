#!/usr/bin/env bash
# Bench fleet helpers: several boards on one machine, each with its own
# Raspberry Pi Debug Probe. Sourced by lib.sh and by device-lock.sh (which
# must stay a standalone CLI, so this file has no dependency on lib.sh).
#
# Pure functions: no top-level side effects, every failure is a non-zero
# return, safe under `set -euo pipefail`.
#
# The fleet is described by a small config file, one row per bench slot:
#
#   slot|probe_serial|board_usb_path|boards[|extras]
#
#   slot            name of the lease (e.g. testbench_rp2350)
#   probe_serial    USB serial of the slot's Debug Probe (probe-rs list)
#   board_usb_path  sysfs position of the board's own USB port, e.g. 1-8.3.3
#                   (the pdb CDC device has no serial, so its physical port
#                   is the only stable identity; the ttyACM name moves)
#   boards          comma list of firmware boards this hardware accepts;
#                   the first one is what the nightly flashes
#   extras          comma list of key=value: cycle=hub (power-cycle the whole
#                   hub instead of the two ports), probe_path=1-8.3.2 (the
#                   probe's sysfs position, used when the probe is powered
#                   off and cannot be found by serial)
#
# Location: ${PICODROID_FLEET_CONF-~/.config/picodroid/fleet.conf}. A missing
# file means "no fleet" and every caller behaves as it did with one board on
# the bench. Set-but-empty disables the fleet explicitly (the test suite).
# See scripts/fleet.conf.example.

FLEET_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FLEET_REPO_ROOT="$(cd "$FLEET_LIB_DIR/.." && pwd)"
FLEET_CONF_DEFAULT="$HOME/.config/picodroid/fleet.conf"
FLEET_UHUBCTL_LOCK="${PICODROID_UHUBCTL_LOCK:-/tmp/picodroid-uhubctl.lock}"
# Raspberry Pi Debug Probe and the picodroid pdb CDC device (the latter is
# pdb_protocol::usb::{VID, PID}; keep in step).
FLEET_PROBE_VIDPID="2e8a:000c"
FLEET_PDB_VIDPID="1209:cdc0"

# ── config ──────────────────────────────────────────────────────────────────

# Prints the active config path; rc 1 when there is no fleet.
fleet_conf_path() {
  local p="${PICODROID_FLEET_CONF-$FLEET_CONF_DEFAULT}"
  [[ -n "$p" && -r "$p" ]] || return 1
  echo "$p"
}
fleet_enabled() { fleet_conf_path >/dev/null; }

# Non-comment rows with trailing whitespace trimmed.
fleet_rows() {
  local conf
  conf=$(fleet_conf_path) || return 1
  grep -vE '^[[:space:]]*(#|$)' "$conf" | sed 's/[[:space:]]*$//'
}
fleet_slots() { fleet_rows | cut -d'|' -f1; }

fleet_slot_row() {
  fleet_rows | awk -F'|' -v s="$1" '$1 == s { print; exit }' | grep .
}

# fleet_slot_field SLOT probe_serial|board_usb_path|boards|extras
fleet_slot_field() {
  local row n
  row=$(fleet_slot_row "$1") || return 1
  case "$2" in
    probe_serial) n=2 ;;
    board_usb_path) n=3 ;;
    boards) n=4 ;;
    extras) n=5 ;;
    *) return 1 ;;
  esac
  cut -d'|' -f"$n" <<<"$row"
}

# fleet_slot_extra SLOT KEY -> the value, or empty.
fleet_slot_extra() {
  local extras
  extras=$(fleet_slot_field "$1" extras) || return 1
  tr ',' '\n' <<<"$extras" | sed -n "s/^$2=//p" | head -1
}

fleet_slot_boards() { fleet_slot_field "$1" boards | tr ',' '\n' | grep . ; }
fleet_slot_primary() { fleet_slot_boards "$1" | head -1; }
fleet_slot_has_board() { fleet_slot_boards "$1" 2>/dev/null | grep -qx "$2"; }

# First slot whose boards list contains BOARD.
fleet_slot_for_board() {
  local s
  for s in $(fleet_slots); do
    if fleet_slot_has_board "$s" "$1"; then echo "$s"; return 0; fi
  done
  return 1
}
fleet_slot_for_serial() {
  fleet_rows | awk -F'|' -v v="$1" '$2 == v { print $1; exit }' | grep .
}
fleet_slot_for_usb_path() {
  fleet_rows | awk -F'|' -v v="$1" '$3 == v { print $1; exit }' | grep .
}

# One line per slot, for error messages.
fleet_list_slots() {
  local s
  for s in $(fleet_slots); do
    echo "  $s  ($(fleet_slot_field "$s" boards | tr ',' ' '))"
  done
}

# Validates the config; prints every problem, rc 1 if any.
fleet_check_conf() {
  local conf
  conf=$(fleet_conf_path) || {
    echo "fleet: no config at ${PICODROID_FLEET_CONF-$FLEET_CONF_DEFAULT}" >&2
    return 1
  }
  local errors=0 seen=$'\n' slot serial path boards extras b kv
  err() { echo "fleet: $conf: $*" >&2; errors=$((errors + 1)); }
  while IFS='|' read -r slot serial path boards extras; do
    if [[ -z "$slot" || -z "$serial" || -z "$path" || -z "$boards" ]]; then
      err "row '$slot|$serial|$path|$boards': needs slot|probe_serial|board_usb_path|boards"
      continue
    fi
    [[ "$slot" =~ ^[A-Za-z0-9_.-]+$ ]] || err "slot '$slot': letters, digits, _ . - only"
    if [[ "$seen" == *$'\n'"$slot"$'\n'* ]]; then err "slot '$slot' listed twice"; fi
    seen+="$slot"$'\n'
    [[ "$path" =~ ^[0-9]+-[0-9]+(\.[0-9]+)*$ ]] \
      || err "slot '$slot': board_usb_path '$path' is not a sysfs position like 1-8.3.3"
    for b in ${boards//,/ }; do
      compgen -G "$FLEET_REPO_ROOT/platforms/*/boards/$b" >/dev/null \
        || err "slot '$slot': board '$b' has no platforms/*/boards/ directory"
    done
    for kv in ${extras//,/ }; do
      case "$kv" in
        cycle=hub|cycle=ports) ;;
        probe_path=*) [[ "${kv#probe_path=}" =~ ^[0-9]+-[0-9]+(\.[0-9]+)*$ ]] \
          || err "slot '$slot': probe_path '${kv#probe_path=}' is not a sysfs position" ;;
        *) err "slot '$slot': unknown extra '$kv' (cycle=hub|ports, probe_path=...)" ;;
      esac
    done
  done < <(fleet_rows)
  unset -f err
  [[ $errors -eq 0 ]]
}

# ── slot resolution ─────────────────────────────────────────────────────────

# fleet_resolve_slot HELD_SLOTS [args...] -> prints the slot a device script
# means. HELD_SLOTS is a newline list of slots this owner already holds.
# First hit wins:
#   --slot S | --board B, -b B, --board=B | --boards B1,B2 (first that maps)
#   | -s /dev/ttyACMn (tty -> usb position -> slot) | $PICODROID_SLOT
#   | $PICODROID_BOARD | the single held slot | the only slot in the config
#   | rc 1 with the slot list on stderr.
fleet_resolve_slot() {
  local held="$1"; shift
  local slot="" board="" boards="" tty="" a
  while [[ $# -gt 0 ]]; do
    a="$1"; shift
    case "$a" in
      --slot) slot="${1:-}"; [[ $# -gt 0 ]] && shift ;;
      --slot=*) slot="${a#--slot=}" ;;
      --board|-b) [[ -n "$board" ]] || board="${1:-}"; [[ $# -gt 0 ]] && shift ;;
      --board=*) [[ -n "$board" ]] || board="${a#--board=}" ;;
      --boards) boards="${1:-}"; [[ $# -gt 0 ]] && shift ;;
      --boards=*) boards="${a#--boards=}" ;;
      -s) tty="${1:-}"; [[ $# -gt 0 ]] && shift ;;
    esac
  done
  local b path
  if [[ -n "$slot" ]]; then
    fleet_slot_row "$slot" >/dev/null || { fleet_resolve_fail "unknown slot '$slot'"; return 1; }
    echo "$slot"; return 0
  fi
  if [[ -n "$board" ]]; then
    slot=$(fleet_slot_for_board "$board") || { fleet_resolve_fail "no slot runs board '$board'"; return 1; }
    echo "$slot"; return 0
  fi
  if [[ -n "$boards" ]]; then
    for b in ${boards//,/ }; do
      if slot=$(fleet_slot_for_board "$b"); then echo "$slot"; return 0; fi
    done
    fleet_resolve_fail "no slot runs any of '$boards'"; return 1
  fi
  if [[ -n "$tty" ]]; then
    if path=$(usb_path_of_tty "$tty") && slot=$(fleet_slot_for_usb_path "$path"); then
      echo "$slot"; return 0
    fi
    fleet_resolve_fail "$tty is not a board in the fleet"; return 1
  fi
  if [[ -n "${PICODROID_SLOT:-}" ]]; then
    fleet_slot_row "$PICODROID_SLOT" >/dev/null \
      || { fleet_resolve_fail "PICODROID_SLOT='$PICODROID_SLOT' is not a slot"; return 1; }
    echo "$PICODROID_SLOT"; return 0
  fi
  if [[ -n "${PICODROID_BOARD:-}" ]]; then
    slot=$(fleet_slot_for_board "$PICODROID_BOARD") \
      || { fleet_resolve_fail "no slot runs PICODROID_BOARD='$PICODROID_BOARD'"; return 1; }
    echo "$slot"; return 0
  fi
  held=$(grep . <<<"$held" || true)
  if [[ -n "$held" ]]; then
    if [[ $(wc -l <<<"$held") -eq 1 ]]; then echo "$held"; return 0; fi
    fleet_resolve_fail "you hold several slots ($(tr '\n' ' ' <<<"$held")) -- say which"; return 1
  fi
  if [[ $(fleet_slots | wc -l) -eq 1 ]]; then fleet_slots; return 0; fi
  fleet_resolve_fail "which board? pass --board NAME (or --slot NAME)"; return 1
}

fleet_resolve_fail() {
  {
    echo "fleet: $*"
    echo "  slots in $(fleet_conf_path):"
    fleet_list_slots
  } >&2
}

# Exports the slot's identity for the calling script: PICODROID_SLOT,
# PICODROID_SLOT_BOARDS, PICODROID_PROBE_SERIAL, PICODROID_BOARD_USB_PATH and
# PROBE_RS_PROBE (VID:PID:SERIAL, what probe-rs --probe takes; every probe-rs
# subcommand reads the variable). The pdb tty is deliberately NOT exported:
# it moves after a power cycle, so callers ask usb_path_tty each time.
fleet_export_slot() {
  local slot="$1" serial
  serial=$(fleet_slot_field "$slot" probe_serial) || return 1
  export PICODROID_SLOT="$slot"
  PICODROID_SLOT_BOARDS="$(fleet_slot_field "$slot" boards)"; export PICODROID_SLOT_BOARDS
  export PICODROID_PROBE_SERIAL="$serial"
  PICODROID_BOARD_USB_PATH="$(fleet_slot_field "$slot" board_usb_path)"; export PICODROID_BOARD_USB_PATH
  PROBE_RS_PROBE="$(probe_selector "$serial")"; export PROBE_RS_PROBE
}

# SERIAL -> VID:PID:SERIAL. Reads the ids from sysfs while the probe is
# enumerated, falls back to the Debug Probe's ids otherwise.
probe_selector() {
  local p
  if p=$(usb_path_for_serial "$1") && [[ -r "/sys/bus/usb/devices/$p/idVendor" ]]; then
    echo "$(cat "/sys/bus/usb/devices/$p/idVendor"):$(cat "/sys/bus/usb/devices/$p/idProduct"):$1"
  else
    echo "$FLEET_PROBE_VIDPID:$1"
  fi
}

# ── sysfs / tty ─────────────────────────────────────────────────────────────

# SERIAL -> sysfs position (e.g. 1-8.3.2); rc 1 when not enumerated.
usb_path_for_serial() {
  local f d
  for f in /sys/bus/usb/devices/*/serial; do
    [[ -r "$f" ]] || continue
    if [[ "$(cat "$f" 2>/dev/null)" == "$1" ]]; then
      d="${f%/serial}"; echo "${d##*/}"; return 0
    fi
  done
  return 1
}

# POSITION -> "HUB PORT" as uhubctl wants them: 1-8.3.2 -> "1-8.3 2";
# a device straight on a root port (1-8) -> "1 8".
usb_hub_port() {
  local p="$1"
  [[ "$p" =~ ^[0-9]+-[0-9]+(\.[0-9]+)*$ ]] || return 1
  if [[ "$p" == *.* ]]; then echo "${p%.*} ${p##*.}"; else echo "${p%-*} ${p#*-}"; fi
}

# POSITION -> /dev/ttyACMn of the CDC device sitting there; rc 1 if absent.
usb_path_tty() {
  local t
  for t in /sys/bus/usb/devices/"$1":1.*/tty/tty*; do
    [[ -e "$t" ]] || continue
    echo "/dev/${t##*/}"; return 0
  done
  return 1
}

# /dev/ttyACMn -> sysfs position of the USB device behind it.
usb_path_of_tty() {
  local name="${1##*/}" dev
  dev=$(readlink -f "/sys/class/tty/$name/device" 2>/dev/null) || return 1
  dev="${dev##*/}"          # 1-8.3.3:1.0
  [[ "$dev" == *:* ]] || return 1
  echo "${dev%%:*}"
}

# ── probe-rs processes ──────────────────────────────────────────────────────

# probe_rs_pids [SERIAL]: this uid's probe-rs pids; with SERIAL only those
# whose environment (PROBE_RS_PROBE=...:SERIAL) or command line
# (--probe ...SERIAL) selects that probe. -x matches the process name only,
# never a shell whose command line mentions probe-rs.
probe_rs_pids() {
  local serial="${1:-}" pid
  for pid in $(pgrep -x -U "$(id -u)" probe-rs 2>/dev/null || true); do
    if [[ -z "$serial" ]]; then echo "$pid"; continue; fi
    if tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | grep -q "^PROBE_RS_PROBE=.*:${serial}\$" \
       || tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null | grep -q -- "--probe [^ ]*${serial}"; then
      echo "$pid"
    fi
  done
}

# kill_probe_rs_scoped [SERIAL]: kills those pids; prints what it killed.
kill_probe_rs_scoped() {
  local pids
  pids=$(probe_rs_pids "${1:-}")
  [[ -n "$pids" ]] || return 0
  # shellcheck disable=SC2086
  kill $pids 2>/dev/null || true
  echo "killed lingering probe-rs (pid $(echo $pids | tr '\n' ' '))"
}

# ── power ───────────────────────────────────────────────────────────────────

# power_cycle_slot SLOT: cycles the slot's probe port and board port (or the
# whole hub with cycle=hub). Prints the uhubctl commands it runs; rc from
# uhubctl. Calls are serialized machine-wide -- two runners must not drive
# uhubctl on one hub at the same time. The caller holds the slot's lease.
power_cycle_slot() {
  local slot="$1" serial bpath ppath mode bhub bport phub pport
  serial=$(fleet_slot_field "$slot" probe_serial) || { echo "fleet: unknown slot '$slot'" >&2; return 1; }
  bpath=$(fleet_slot_field "$slot" board_usb_path)
  mode=$(fleet_slot_extra "$slot" cycle); mode="${mode:-ports}"
  ppath=$(fleet_slot_extra "$slot" probe_path)
  [[ -n "$ppath" ]] || ppath=$(usb_path_for_serial "$serial") || ppath=""
  read -r bhub bport < <(usb_hub_port "$bpath") || return 1
  phub=""; pport=""
  if [[ -n "$ppath" ]]; then
    read -r phub pport < <(usb_hub_port "$ppath") || return 1
  else
    echo "fleet: probe $serial is not enumerated and $slot has no probe_path=; cycling the board port only" >&2
  fi
  local -a cmds=()
  if [[ "$mode" == "hub" ]]; then
    cmds+=("-l $bhub -a cycle")
    [[ -n "$phub" && "$phub" != "$bhub" ]] && cmds+=("-l $phub -a cycle")
  elif [[ -n "$phub" && "$phub" == "$bhub" ]]; then
    cmds+=("-l $bhub -p $(printf '%s\n' "$pport" "$bport" | sort -n | paste -sd,) -a cycle")
  else
    cmds+=("-l $bhub -p $bport -a cycle")
    [[ -n "$phub" ]] && cmds+=("-l $phub -p $pport -a cycle")
  fi
  local c rc=0
  (
    flock -w 120 9 || { echo "fleet: uhubctl lock $FLEET_UHUBCTL_LOCK busy" >&2; exit 1; }
    for c in "${cmds[@]}"; do
      echo "uhubctl $c"
      # shellcheck disable=SC2086
      sudo uhubctl $c || exit $?
    done
  ) 9>"$FLEET_UHUBCTL_LOCK" || rc=$?
  return $rc
}

# ── discovery ───────────────────────────────────────────────────────────────

# Bring-up report: every Debug Probe and every picodroid CDC device on the
# host with its sysfs position, hub/port and (if configured) slot.
fleet_discover() {
  local d name vp serial hub port tty slot
  echo "Debug probes ($FLEET_PROBE_VIDPID):"
  for d in /sys/bus/usb/devices/*; do
    [[ -r "$d/idVendor" && -r "$d/idProduct" ]] || continue
    vp="$(cat "$d/idVendor"):$(cat "$d/idProduct")"
    [[ "$vp" == "$FLEET_PROBE_VIDPID" ]] || continue
    name="${d##*/}"
    serial="$(cat "$d/serial" 2>/dev/null || echo '?')"
    read -r hub port < <(usb_hub_port "$name" || echo "? ?")
    slot="$(fleet_slot_for_serial "$serial" 2>/dev/null || echo '-')"
    printf '  serial %-18s usb %-10s hub %-8s port %-2s slot %s\n' "$serial" "$name" "$hub" "$port" "$slot"
  done
  echo "Picodroid boards ($FLEET_PDB_VIDPID):"
  for d in /sys/bus/usb/devices/*; do
    [[ -r "$d/idVendor" && -r "$d/idProduct" ]] || continue
    vp="$(cat "$d/idVendor"):$(cat "$d/idProduct")"
    [[ "$vp" == "$FLEET_PDB_VIDPID" ]] || continue
    name="${d##*/}"
    tty="$(usb_path_tty "$name" || echo '-')"
    read -r hub port < <(usb_hub_port "$name" || echo "? ?")
    slot="$(fleet_slot_for_usb_path "$name" 2>/dev/null || echo '-')"
    printf '  usb %-10s tty %-14s hub %-8s port %-2s slot %s\n' "$name" "$tty" "$hub" "$port" "$slot"
  done
  echo "Config: $(fleet_conf_path 2>/dev/null || echo "none (${PICODROID_FLEET_CONF-$FLEET_CONF_DEFAULT})")"
}
