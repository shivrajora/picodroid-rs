#!/usr/bin/env bash
# Locks: the repo-wide Gradle lock and the per-board device lease.
#
# Sourced by lib.sh (which sets REPO_ROOT and SCRIPT_DIR); do not run or
# source directly. Functions only -- no work happens at source time.

# Runs a command holding the repo-wide Gradle lock.
#
# pre-commit fans its stages out across parallel lanes, and two Gradle
# invocations against one project directory contend on Gradle's own project
# lock -- at best blocking, at worst the papk race that produces a
# FrameworkVersionMismatch at `pdb install`. Every gradlew entry point goes
# through here so at most one is ever live.
#
# The timeout is a deadlock detector, not a tuning knob: nothing in this repo
# should hold the lock for ten minutes, and a nested acquisition (a Gradle task
# invoking build-apk.sh without PICODROID_SKIP_GRADLE=1) would otherwise hang
# forever with no clue why. flock is util-linux; without it, run unlocked --
# callers that care run pre-commit --serial.
gradle_lock_run() {
  mkdir -p "$REPO_ROOT/build"
  if command -v flock >/dev/null 2>&1; then
    flock -w 600 "$REPO_ROOT/build/.gradle.lock" "$@"
  else
    "$@"
  fi
}

# The same lock, held by the calling shell across several commands (fd 9):
# build-apk.sh keeps it from gradlew through the copy of Gradle's output,
# because that output file is shared by every caller and two parallel
# builds of one app in different shrink modes would otherwise swap papks.
# Commands run while it is held must close fd 9 (`9>&-`) so a Gradle daemon
# cannot inherit and keep it. Same 600 s deadlock detector as above.
GRADLE_LOCK_HELD=0
gradle_lock_acquire() {
  mkdir -p "$REPO_ROOT/build"
  command -v flock >/dev/null 2>&1 || return 0
  exec 9>"$REPO_ROOT/build/.gradle.lock"
  flock -w 600 9 || { echo "gradle lock: could not take build/.gradle.lock within 600 s" >&2; return 1; }
  GRADLE_LOCK_HELD=1
}
gradle_lock_release() {
  [[ "$GRADLE_LOCK_HELD" == 1 ]] || return 0
  flock -u 9
  exec 9>&-
  GRADLE_LOCK_HELD=0
}

# Takes the lease on a dev board for the caller's session, or exits 75
# (EX_TEMPFAIL) with the holder and a hint.
#
# Several parallel sessions, one lease per board: every script that flashes,
# power-cycles or talks pdb to a board calls this first. If the board is
# free the lease is taken and kept -- it belongs to the *session* (inside
# Claude the claude process, in a terminal the shell that ran the script),
# not to this command, so a flash followed by pdb calls needs no ceremony and
# nothing can interleave. Release with `./scripts/device-lock.sh release`.
# The lease evaporates on its own when the owning process exits.
#
# Optional leading `--wait SECS` queues instead of failing. The remaining
# args label the lease in `status`; with a fleet config they are also
# scanned for the board (--slot NAME, --board/-b NAME, --boards A,B, or
# -s /dev/ttyACMn), falling back to the slot this session already holds or
# the only slot configured (fleet-lib.sh::fleet_resolve_slot). In fleet mode
# the slot's identity is exported for the caller: PICODROID_SLOT,
# PICODROID_PROBE_SERIAL, PICODROID_BOARD_USB_PATH and PROBE_RS_PROBE, so
# probe-rs talks to this slot's probe and nothing else.
#
# PICODROID_DEVICE_LOCK=0 skips the check (emergencies) but still resolves
# the slot. Without flock the check is skipped too, matching gradle_lock_run.
require_device_lock() {
  # $PPID in a sourced function is the parent of the script, i.e. the shell
  # (or Claude session) that launched it -- the lease must outlive the script.
  local owner_pid="${PICODROID_DEVICE_OWNER_PID:-${CLAUDE_PID:-$PPID}}"
  local -a slot_args=()
  if fleet_enabled; then
    local held slot
    held="$(PICODROID_DEVICE_OWNER_PID="$owner_pid" bash "$SCRIPT_DIR/device-lock.sh" mine 2>/dev/null || true)"
    slot=$(fleet_resolve_slot "$held" "$@") || exit 1
    fleet_export_slot "$slot"
    slot_args=(--slot "$slot")
  fi
  if [[ "${PICODROID_DEVICE_LOCK:-1}" == "0" ]]; then
    echo "WARNING: PICODROID_DEVICE_LOCK=0 -- touching the board without the device lock" >&2
    return 0
  fi
  if ! command -v flock >/dev/null 2>&1; then
    echo "WARNING: flock not found -- touching the board without the device lock" >&2
    return 0
  fi
  local wait_args=()
  if [[ "${1:-}" == "--wait" ]]; then
    wait_args=(--wait "${2:-}")
    shift 2
  fi
  PICODROID_DEVICE_OWNER_PID="$owner_pid" \
    bash "$SCRIPT_DIR/device-lock.sh" acquire ${slot_args[@]+"${slot_args[@]}"} \
      ${wait_args[@]+"${wait_args[@]}"} \
      --note "$(basename "$0") $*" \
    || exit $?
}
