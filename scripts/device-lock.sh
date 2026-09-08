#!/usr/bin/env bash
# Machine-wide lease on a dev board.
#
# One probe per board, several parallel sessions (Claude Code sessions in
# worktrees, a terminal, the 4 AM hil-fleet cron). Whoever holds the lease may
# flash, power-cycle or talk pdb to that board; everyone else fails fast with
# exit 75 or queues with `acquire --wait` and is handed the board in FIFO
# order. Every device script calls lib.sh::require_device_lock, which
# auto-acquires when the board is free -- so in the common case nothing has
# to be done by hand except `release` when you are finished.
#
#   ./scripts/device-lock.sh status [--slot NAME]
#   ./scripts/device-lock.sh acquire [--slot NAME|--board NAME] [--wait [SECS]] [--note TEXT]
#   ./scripts/device-lock.sh release [--slot NAME] [--keep-probe]
#   ./scripts/device-lock.sh run [--slot NAME] [--wait SECS] -- ./scripts/flash.sh --app x
#   ./scripts/device-lock.sh break --slot NAME --force
#   ./scripts/device-lock.sh mine
#
# Fleet. With a fleet config (scripts/fleet-lib.sh, one row per board slot)
# there is one lease per slot, kept in slots/<slot>/ under the state
# directory. --slot names it directly, --board picks the slot whose boards
# list contains that firmware board; with neither, `acquire`/`run` take the
# one slot this session already holds, or the only slot in the config, or
# refuse and list the slots. `status` with no slot shows every slot;
# `release` with no slot gives back every slot you hold. Without a fleet
# config there is exactly one lease, as there always was, and --slot/--board
# are refused.
#
# Identity. The lease belongs to a *session*, not to a command. Inside Claude
# Code every Bash call carries CLAUDE_CODE_SESSION_ID and CLAUDE_PID (the
# session's own claude process), so the owner is `claude:<8 chars of the id>`
# and the lease is alive exactly as long as that process is. Outside Claude
# the owner is `shell:<user>:<pid of your shell>`. Either can be overridden:
#
#   PICODROID_DEVICE_OWNER      owner name (hil-run, soak, ...)
#   PICODROID_DEVICE_OWNER_PID  process whose death releases the lease
#
# State lives in ${PICODROID_DEVICE_LOCK_DIR:-/tmp/picodroid-device-lock}
# (per slot: slots/<slot>/ below it):
#   meta.lock   the only flock, held for milliseconds around each read/write
#   holder      key=value lines describing the lease (written tmp + mv)
#   queue/      one ticket per waiter, <counter>-<pid>, FIFO by counter
#   seq         the ticket counter
#
# There is no daemon and no long-held kernel lock: the board is held iff
# `holder` exists and (it is pinned, or its live pid is alive with the start
# time recorded at acquire). Anything else is stale and is swept by whoever
# looks next. Pid reuse cannot resurrect a dead lease because the start time
# must match too.
set -euo pipefail

SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
# shellcheck source=fleet-lib.sh
source "$(dirname "$SELF")/fleet-lib.sh"
LOCK_DIR="${PICODROID_DEVICE_LOCK_DIR:-/tmp/picodroid-device-lock}"
POLL="${PICODROID_DEVICE_LOCK_POLL:-1}"
TICKET_STALE_S=30        # a waiter touches its ticket every poll
DEFAULT_WAIT_S=1800
PROGRESS_EVERY_S=30
EX_BUSY=75               # EX_TEMPFAIL: try again later

# The slot this invocation acts on ("" = the single legacy lease) and the
# state files that go with it.
SLOT=""
select_state_dir() {
  SLOT="${1:-}"
  local state="$LOCK_DIR"
  [[ -n "$SLOT" ]] && state="$LOCK_DIR/slots/$SLOT"
  QUEUE_DIR="$state/queue"
  HOLDER="$state/holder"
  META="$state/meta.lock"
  SEQ="$state/seq"
}
select_state_dir ""

# Message prefix: "device lock" or "device lock [slot]".
tag() { echo "device lock${SLOT:+ [$SLOT]}"; }

usage() {
  cat <<EOF
Usage: $(basename "$0") <command> [options]

Commands:
  status  [--quiet|--short]   Show the holder and the queue. --quiet prints
                              nothing and exits 0 (free) / 1 (held). With a
                              fleet and no --slot: every slot.
  acquire [--wait [SECS]] [--note TEXT] [--pin] [--owner NAME]
                              Take the lease for this session. Without --wait
                              it fails with exit $EX_BUSY when the board is busy
                              or someone is queued. --wait queues (FIFO) for up
                              to SECS (default $DEFAULT_WAIT_S). --pin makes a lease
                              that outlives every process (soaks); it needs an
                              explicit --owner and only release/break free it.
  release [--keep-probe]      Give the board back. Also kills the probe-rs
                              still attached to its probe (yours by
                              definition) unless --keep-probe. With a fleet
                              and no --slot: every slot you hold.
  run     [--wait SECS] [--note TEXT] -- CMD...
                              Acquire, run CMD, release; the lease is tied to
                              this process. If this session already holds the
                              lease CMD just runs inside it.
  break   --force             Evict whoever holds the lease.
  mine                        Print the slots this session holds (fleet only).

Options accepted by every command (fleet config only):
  --slot NAME                 The bench slot to act on.
  --board NAME                The slot whose boards list contains NAME.

Exit codes: 0 ok, 1 error, $EX_BUSY busy (holder / queue printed on stderr).

Environment:
  PICODROID_DEVICE_OWNER       owner name (default claude:<session> or shell:<user>:<pid>)
  PICODROID_DEVICE_OWNER_PID   liveness pid (default \$CLAUDE_PID or the calling shell)
  PICODROID_DEVICE_LOCK_DIR    state directory (default /tmp/picodroid-device-lock)
  PICODROID_DEVICE_LOCK_POLL   wait poll interval in seconds (default 1)
  PICODROID_FLEET_CONF         fleet config (default ~/.config/picodroid/fleet.conf)
EOF
}

die() { echo "$(tag): $*" >&2; exit 1; }

# ── process liveness ────────────────────────────────────────────────────────

# Start time (clock ticks since boot) of a pid, or empty if it is gone.
# /proc/<pid>/stat field 22; the comm field may contain spaces so cut past ')'.
proc_start() {
  local rest
  rest=$(sed 's/^.*) //' "/proc/$1/stat" 2>/dev/null) || return 1
  # shellcheck disable=SC2086  # word-splitting the stat line is the point
  set -- $rest
  echo "${20:-}"
}

# is_live PID START -> 0 when PID exists and was started at START.
is_live() {
  local pid="$1" start="$2"
  [[ -n "$pid" && -n "$start" && -d "/proc/$pid" ]] || return 1
  [[ "$(proc_start "$pid")" == "$start" ]]
}

# ── identity ────────────────────────────────────────────────────────────────

OWNER_OPT=""
resolve_identity() {
  LIVE_PID="${PICODROID_DEVICE_OWNER_PID:-${CLAUDE_PID:-$PPID}}"
  OWNER="${OWNER_OPT:-${PICODROID_DEVICE_OWNER:-}}"
  if [[ -z "$OWNER" ]]; then
    if [[ -n "${CLAUDE_CODE_SESSION_ID:-}" ]]; then
      OWNER="claude:${CLAUDE_CODE_SESSION_ID:0:8}"
    else
      OWNER="shell:${USER:-$(id -un)}:$LIVE_PID"
    fi
  fi
  LIVE_START="$(proc_start "$LIVE_PID" || true)"
}

# ── metadata lock ───────────────────────────────────────────────────────────

meta_lock() {
  mkdir -p "$QUEUE_DIR"
  exec 8>"$META"
  flock -w 5 8 || die "could not take $META within 5 s"
}
meta_unlock() { flock -u 8; }

# ── holder file ─────────────────────────────────────────────────────────────

# Populates H_* from the holder file. Returns 1 if absent or garbage. The
# file is parsed line by line, never sourced: the note is user text.
read_holder() {
  H_OWNER="" H_LIVE_PID="" H_LIVE_START="" H_PINNED="0" H_NOTE="" H_SINCE="" H_CWD="" H_BRANCH=""
  [[ -f "$HOLDER" ]] || return 1
  local k v
  while IFS='=' read -r k v; do
    case "$k" in
      owner)      H_OWNER="$v" ;;
      live_pid)   H_LIVE_PID="$v" ;;
      live_start) H_LIVE_START="$v" ;;
      pinned)     H_PINNED="$v" ;;
      note)       H_NOTE="$v" ;;
      since)      H_SINCE="$v" ;;
      cwd)        H_CWD="$v" ;;
      branch)     H_BRANCH="$v" ;;
    esac
  done < "$HOLDER"
  [[ -n "$H_OWNER" ]]
}

# write_holder OWNER LIVE_PID LIVE_START PINNED NOTE SINCE CWD BRANCH
write_holder() {
  local tmp="$HOLDER.tmp.$$"
  {
    echo "owner=$1"
    echo "live_pid=$2"
    echo "live_start=$3"
    echo "pinned=$4"
    echo "note=$(printf '%s' "$5" | tr -d '\n')"
    echo "since=$6"
    echo "cwd=$7"
    echo "branch=$8"
  } > "$tmp"
  mv -f "$tmp" "$HOLDER"
}

holder_valid() {
  read_holder || return 1
  [[ "$H_PINNED" == "1" ]] && return 0
  is_live "$H_LIVE_PID" "$H_LIVE_START"
}

# Drop a stale holder file and dead / abandoned tickets. Call under meta_lock.
sweep() {
  if [[ -f "$HOLDER" ]] && ! holder_valid; then
    rm -f "$HOLDER"
  fi
  local t pid start now mtime
  now=$(date +%s)
  for t in "$QUEUE_DIR"/*; do
    [[ -e "$t" ]] || continue
    pid="${t##*-}"
    start="$(sed -n '2p' "$t" 2>/dev/null || true)"
    mtime=$(stat -c %Y "$t" 2>/dev/null || echo 0)
    if ! is_live "$pid" "$start" || (( now - mtime > TICKET_STALE_S )); then
      rm -f "$t"
    fi
  done
}

# Sorted ticket basenames, one per line.
queue_list() {
  local t
  for t in "$QUEUE_DIR"/*; do
    [[ -e "$t" ]] && basename "$t"
  done | sort
}

queue_owner() { sed -n '1p' "$QUEUE_DIR/$1" 2>/dev/null || true; }

take_lease() {
  local note="$1" pinned="$2" branch
  branch="$(git branch --show-current 2>/dev/null || true)"
  write_holder "$OWNER" "$LIVE_PID" "$LIVE_START" "$pinned" \
    "$note" "$(date +%s)" "$PWD" "$branch"
}

# ── fleet slots ─────────────────────────────────────────────────────────────

GLOBAL_SLOT_OPT=""
GLOBAL_BOARD_OPT=""

# Slots (fleet config order) whose lease belongs to $OWNER. Reads without the
# metadata lock: a stale holder simply fails holder_valid.
held_slots() {
  fleet_enabled || return 0
  local s saved="$HOLDER"
  for s in $(fleet_slots); do
    HOLDER="$LOCK_DIR/slots/$s/holder"
    if holder_valid && [[ "$H_OWNER" == "$OWNER" ]]; then echo "$s"; fi
  done
  HOLDER="$saved"
}

# Pick the slot for this invocation and point the state files at it. In
# legacy mode (no fleet config) --slot/--board are refused. `with_held`
# lets a session that already holds one slot omit the name.
resolve_slot() {
  local with_held="${1:-1}"
  if ! fleet_enabled; then
    [[ -z "$GLOBAL_SLOT_OPT$GLOBAL_BOARD_OPT" ]] \
      || die "--slot/--board need a fleet config (${PICODROID_FLEET_CONF-$FLEET_CONF_DEFAULT})"
    return 0
  fi
  local -a hint=()
  [[ -n "$GLOBAL_SLOT_OPT" ]] && hint+=(--slot "$GLOBAL_SLOT_OPT")
  [[ -n "$GLOBAL_BOARD_OPT" ]] && hint+=(--board "$GLOBAL_BOARD_OPT")
  local held="" s
  (( with_held )) && held="$(held_slots)"
  s=$(fleet_resolve_slot "$held" ${hint[@]+"${hint[@]}"}) || exit 1
  select_state_dir "$s"
}

# A session running scripts from before the fleet (a worktree on an older
# branch) takes the single legacy lease at $LOCK_DIR/holder and power-cycles
# the whole hub, so while it is alive no slot is safe to touch. Prints its
# holder line; rc 1 when there is none. Only consulted in fleet mode.
legacy_holder_line() {
  [[ -n "$SLOT" ]] || return 1
  local saved="$HOLDER" rc=1
  HOLDER="$LOCK_DIR/holder"
  if holder_valid; then rc=0; holder_line; fi
  HOLDER="$saved"
  return $rc
}

# The probe serial of the current slot, for scoping probe-rs kills.
slot_serial() {
  [[ -n "$SLOT" ]] || return 0
  fleet_slot_field "$SLOT" probe_serial 2>/dev/null || true
}

# ── display ─────────────────────────────────────────────────────────────────

age_str() {
  local s=$(( $(date +%s) - ${1:-0} ))
  if (( s < 90 )); then echo "${s}s"
  elif (( s < 5400 )); then echo "$(( s / 60 )) min"
  else echo "$(( s / 3600 )) h $(( (s % 3600) / 60 )) min"
  fi
}

holder_line() {
  # One line: "held by OWNER (NOTE) since AGE" -- for scripts and logs.
  if read_holder; then
    local live="alive"
    [[ "$H_PINNED" == "1" ]] && live="pinned"
    echo "held by $H_OWNER${H_NOTE:+ ($H_NOTE)} since $(age_str "$H_SINCE"), $live"
  else
    echo "free"
  fi
}

print_queue() {
  local i=0 t
  while read -r t; do
    [[ -n "$t" ]] || continue
    i=$((i + 1))
    printf '  %s) %s\n' "$i" "$(queue_owner "$t")"
  done < <(queue_list)
  return 0
}

print_status() {
  local boards=""
  [[ -n "$SLOT" ]] && boards=" ($(fleet_slot_field "$SLOT" boards 2>/dev/null | tr ',' ' '))"
  if read_holder; then
    local live
    if [[ "$H_PINNED" == "1" ]]; then live="pinned, survives every process"
    else live="alive, pid $H_LIVE_PID"; fi
    echo "$(tag)$boards: HELD by $H_OWNER  ($live)"
    [[ -n "$H_NOTE" ]] && echo "  note:   $H_NOTE"
    echo "  since:  $(date -d "@$H_SINCE" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || echo "$H_SINCE") ($(age_str "$H_SINCE") ago)"
    [[ -n "$H_CWD" ]] && echo "  cwd:    $H_CWD${H_BRANCH:+  [$H_BRANCH]}"
  else
    echo "$(tag)$boards: FREE"
  fi
  if [[ -n "$(queue_list)" ]]; then
    echo "  queue:"
    print_queue
  fi
}

busy_report() {
  {
    local legacy
    if legacy=$(legacy_holder_line); then
      echo "$(tag): busy -- the whole bench is $legacy (a session without the fleet code)"
    elif read_holder; then
      echo "$(tag): busy -- $(holder_line)"
    else
      echo "$(tag): free, but sessions are queued for it (they go first)"
    fi
    if [[ -n "$(queue_list)" ]]; then
      echo "  waiting:"
      print_queue
    fi
    echo "  you are: $OWNER"
    echo "  hint:    ./scripts/device-lock.sh acquire${SLOT:+ --slot $SLOT} --wait    # queues FIFO; in Claude run it with run_in_background"
    echo "           ./scripts/device-lock.sh status"
  } >&2
}

# ── commands ────────────────────────────────────────────────────────────────

# status_one MODE: the current slot under its own metadata lock.
status_one() {
  meta_lock
  sweep
  case "$1" in
    quiet) if read_holder; then meta_unlock; return 1; fi ;;
    short) echo "${SLOT:+$SLOT: }$(holder_line)" ;;
    full)  print_status ;;
  esac
  meta_unlock
}

cmd_status() {
  local mode="full"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --quiet) mode="quiet" ;;
      --short) mode="short" ;;
      *) die "status: unknown option $1" ;;
    esac
    shift
  done
  if fleet_enabled && [[ -z "$GLOBAL_SLOT_OPT$GLOBAL_BOARD_OPT" ]]; then
    # Every slot; --quiet is 0 only when all are free. A legacy lease (a
    # session without the fleet code) blocks the whole bench.
    local s rc=0 legacy
    select_state_dir "$(fleet_slots | head -1)"
    if legacy=$(legacy_holder_line); then
      rc=1
      case "$mode" in
        short) echo "bench: $legacy (legacy single-board lease)" ;;
        full)  echo "device lock (legacy single-board lease, blocks every slot): $legacy" ;;
      esac
    fi
    for s in $(fleet_slots); do
      select_state_dir "$s"
      status_one "$mode" || rc=1
    done
    exit "$rc"
  fi
  resolve_slot 0
  status_one "$mode" || exit 1
}

cmd_acquire() {
  local wait_s="" note="" pin=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --wait)
        wait_s="$DEFAULT_WAIT_S"
        if [[ "${2:-}" =~ ^[0-9]+$ ]]; then wait_s="$2"; shift; fi
        ;;
      --wait=*) wait_s="${1#--wait=}" ;;
      --no-wait) wait_s="" ;;
      --note) note="${2:-}"; shift ;;
      --note=*) note="${1#--note=}" ;;
      --pin) pin=1 ;;
      --owner) OWNER_OPT="${2:-}"; shift ;;
      --owner=*) OWNER_OPT="${1#--owner=}" ;;
      *) die "acquire: unknown option $1" ;;
    esac
    shift
  done

  if (( pin )); then
    # A pinned lease must not borrow the launching session's identity, or a
    # soak started from Claude would evaporate when that session closes.
    [[ -n "${OWNER_OPT:-${PICODROID_DEVICE_OWNER:-}}" ]] \
      || die "--pin needs an explicit --owner NAME (or PICODROID_DEVICE_OWNER)"
    OWNER="${OWNER_OPT:-$PICODROID_DEVICE_OWNER}"
    LIVE_PID="" LIVE_START=""
  else
    resolve_identity
    [[ -n "$LIVE_START" ]] || die "owner pid $LIVE_PID is not alive (PICODROID_DEVICE_OWNER_PID / CLAUDE_PID)"
  fi
  resolve_slot

  meta_lock
  sweep
  if holder_valid && [[ "$H_OWNER" == "$OWNER" ]]; then
    # Already ours: refresh the note, keep everything else (pinned stays pinned).
    [[ -n "$note" ]] && write_holder "$H_OWNER" "$H_LIVE_PID" "$H_LIVE_START" "$H_PINNED" \
      "$note" "$H_SINCE" "$H_CWD" "$H_BRANCH"
    meta_unlock
    echo "$(tag): already held by $OWNER"
    return 0
  fi

  if [[ -z "$wait_s" ]]; then
    if holder_valid || [[ -n "$(queue_list)" ]] || legacy_holder_line >/dev/null; then
      busy_report
      meta_unlock
      exit "$EX_BUSY"
    fi
    take_lease "$note" "$pin"
    meta_unlock
    echo "$(tag): acquired by $OWNER${note:+ ($note)}"
    return 0
  fi

  # Queue up. The ticket names this process; sweep() drops it if we die or
  # stop touching it, so a killed waiter never blocks the line.
  local n ticket
  n=$(( $(cat "$SEQ" 2>/dev/null || echo 0) + 1 ))
  echo "$n" > "$SEQ"
  ticket="$QUEUE_DIR/$(printf '%08d' "$n")-$$"
  printf '%s\n%s\n' "$OWNER" "$(proc_start $$)" > "$ticket"
  # shellcheck disable=SC2064  # expand the ticket path now
  trap "rm -f '$ticket'" EXIT
  local first=1 pos
  if holder_valid; then
    echo "$(tag): waiting for $(holder_line)"
  fi
  meta_unlock

  local deadline=$(( $(date +%s) + wait_s )) last_report=0 now
  while :; do
    meta_lock
    touch "$ticket"
    sweep
    if ! holder_valid && ! legacy_holder_line >/dev/null \
       && [[ "$(queue_list | head -1)" == "$(basename "$ticket")" ]]; then
      take_lease "$note" 0
      rm -f "$ticket"
      meta_unlock
      trap - EXIT
      echo "$(tag): acquired by $OWNER${note:+ ($note)}"
      return 0
    fi
    now=$(date +%s)
    if (( now - last_report >= PROGRESS_EVERY_S )) || (( first )); then
      pos=$(queue_list | grep -n -x "$(basename "$ticket")" | cut -d: -f1 || true)
      echo "$(tag): queued (position ${pos:-?}) -- $(legacy_holder_line || holder_line)"
      last_report=$now; first=0
    fi
    meta_unlock
    if (( now >= deadline )); then
      echo "$(tag): gave up after ${wait_s}s -- $(holder_line)" >&2
      exit "$EX_BUSY"
    fi
    sleep "$POLL"
  done
}

kill_probe() {
  # Any probe-rs still attached to this slot's probe belongs to the lease
  # that is being given up (or is a leftover), and a lingering one makes the
  # next attempt fail with "Failed to open probe". With a fleet the kill is
  # scoped by the probe serial (fleet-lib.sh::probe_rs_pids), so a sibling
  # board's attach survives; without one it is every probe-rs of this uid.
  # PICODROID_DEVICE_LOCK_KEEP_PROBE=1 is the test suite's guard against
  # killing a real attach while it exercises release/break.
  [[ "${PICODROID_DEVICE_LOCK_KEEP_PROBE:-0}" == "1" ]] && return 0
  local out
  out=$(kill_probe_rs_scoped "$(slot_serial)")
  [[ -n "$out" ]] && echo "$(tag): $out"
  return 0
}

# Release the current slot's lease if it is ours. Prints; rc 1 if not ours.
release_one() {
  local keep_probe="$1"
  meta_lock
  sweep
  if ! read_holder; then
    meta_unlock
    echo "$(tag): already free"
    return 0
  fi
  if [[ "$H_OWNER" != "$OWNER" ]]; then
    meta_unlock
    echo "$(tag): not yours -- $(holder_line); you are $OWNER (use 'break --force' to evict)" >&2
    return 1
  fi
  rm -f "$HOLDER"
  meta_unlock
  echo "$(tag): released by $OWNER"
  (( keep_probe )) || kill_probe
}

cmd_release() {
  local keep_probe=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --keep-probe) keep_probe=1 ;;
      --owner) OWNER_OPT="${2:-}"; shift ;;
      --owner=*) OWNER_OPT="${1#--owner=}" ;;
      *) die "release: unknown option $1" ;;
    esac
    shift
  done
  resolve_identity
  if fleet_enabled && [[ -z "$GLOBAL_SLOT_OPT$GLOBAL_BOARD_OPT" ]]; then
    # Everything this session holds.
    local held s
    held="$(held_slots)"
    if [[ -z "$held" ]]; then
      echo "device lock: nothing held by $OWNER"
      return 0
    fi
    for s in $held; do
      select_state_dir "$s"
      release_one "$keep_probe"
    done
    return 0
  fi
  resolve_slot
  release_one "$keep_probe" || exit 1
}

cmd_break() {
  local force=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --force) force=1 ;;
      *) die "break: unknown option $1" ;;
    esac
    shift
  done
  # Evicting is about someone else's slot, so "the slot I hold" is no hint.
  resolve_slot 0
  meta_lock
  sweep
  if ! read_holder; then
    meta_unlock
    echo "$(tag): already free"
    return 0
  fi
  if ! (( force )); then
    meta_unlock
    die "refusing to evict without --force -- $(holder_line)"
  fi
  echo "$(tag): evicting $(holder_line)"
  rm -f "$HOLDER"
  meta_unlock
  kill_probe
}

cmd_mine() {
  [[ $# -eq 0 ]] || die "mine: takes no options"
  resolve_identity
  held_slots
}

cmd_run() {
  local wait_s="" note="" cmd=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --wait) wait_s="${2:-}"; shift ;;
      --wait=*) wait_s="${1#--wait=}" ;;
      --note) note="${2:-}"; shift ;;
      --note=*) note="${1#--note=}" ;;
      --owner) OWNER_OPT="${2:-}"; shift ;;
      --owner=*) OWNER_OPT="${1#--owner=}" ;;
      --) shift; cmd=("$@"); break ;;
      *) die "run: unknown option $1 (put the command after --)" ;;
    esac
    shift
  done
  (( ${#cmd[@]} )) || die "run: no command given after --"

  # Already inside a lease of this session? Then just run nested.
  resolve_identity
  resolve_slot
  # Children resolve the same slot and talk to the same probe.
  if [[ -n "$SLOT" ]]; then fleet_export_slot "$SLOT"; fi
  meta_lock; sweep
  local rc=0
  if holder_valid && [[ "$H_OWNER" == "$OWNER" ]]; then
    meta_unlock
    "${cmd[@]}" 8>&- || rc=$?
    return "$rc"
  fi
  meta_unlock

  # Otherwise the lease belongs to this very process, and every child sees
  # the same identity so nested require_device_lock calls are idempotent.
  export PICODROID_DEVICE_OWNER="${OWNER_OPT:-${PICODROID_DEVICE_OWNER:-run:${USER:-$(id -un)}:$$}}"
  export PICODROID_DEVICE_OWNER_PID="$$"
  OWNER_OPT=""
  [[ -n "$note" ]] || note="${cmd[*]}"
  local acq=(acquire --note "$note")
  [[ -n "$SLOT" ]] && acq+=(--slot "$SLOT")
  [[ -n "$wait_s" ]] && acq+=(--wait "$wait_s")
  "$SELF" "${acq[@]}"

  local child=""
  # shellcheck disable=SC2064  # expand the slot now
  trap "'$SELF' release ${SLOT:+--slot '$SLOT'} >/dev/null 2>&1 || true" EXIT
  trap '[[ -n "$child" ]] && kill -TERM "$child" 2>/dev/null' INT TERM
  "${cmd[@]}" 8>&- &
  child=$!
  wait "$child" || rc=$?
  return "$rc"
}

# ── main ────────────────────────────────────────────────────────────────────

command -v flock >/dev/null 2>&1 || die "flock (util-linux) is required"

# --slot / --board are accepted anywhere on the line; everything else goes
# to the subcommand.
args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --slot) GLOBAL_SLOT_OPT="${2:-}"; [[ $# -gt 1 ]] && shift ;;
    --slot=*) GLOBAL_SLOT_OPT="${1#--slot=}" ;;
    --board) GLOBAL_BOARD_OPT="${2:-}"; [[ $# -gt 1 ]] && shift ;;
    --board=*) GLOBAL_BOARD_OPT="${1#--board=}" ;;
    *) args+=("$1") ;;
  esac
  shift
done
set -- ${args[@]+"${args[@]}"}

[[ $# -ge 1 ]] || { usage; exit 1; }
cmd="$1"; shift
case "$cmd" in
  status)  cmd_status "$@" ;;
  acquire) cmd_acquire "$@" ;;
  release) cmd_release "$@" ;;
  break)   cmd_break "$@" ;;
  run)     cmd_run "$@" ;;
  mine)    cmd_mine "$@" ;;
  -h|--help|help) usage ;;
  *) usage; die "unknown command: $cmd" ;;
esac
