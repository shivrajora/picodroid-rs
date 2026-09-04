#!/usr/bin/env bash
# State-machine test for scripts/device-lock.sh and lib.sh::require_device_lock.
#
# Runs against a private lock directory with `sleep` processes standing in for
# sessions, so it never touches the real lease in /tmp/picodroid-device-lock
# and never kills a real probe-rs (PICODROID_DEVICE_LOCK_KEEP_PROBE=1 below;
# the probe-kill path is opt-in via PICODROID_DEVICE_LOCK_TEST_PROBE=1).
#
#   bash scripts/test-device-lock.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DL="$SCRIPT_DIR/device-lock.sh"

export PICODROID_DEVICE_LOCK_DIR
PICODROID_DEVICE_LOCK_DIR="$(mktemp -d /tmp/device-lock-test.XXXXXX)"
export PICODROID_DEVICE_LOCK_POLL=0.2
export PICODROID_DEVICE_LOCK_KEEP_PROBE=1
unset CLAUDE_PID CLAUDE_CODE_SESSION_ID PICODROID_DEVICE_OWNER \
      PICODROID_DEVICE_OWNER_PID PICODROID_DEVICE_LOCK
DIR="$PICODROID_DEVICE_LOCK_DIR"
HOLDER="$DIR/holder"

declare -A PID
cleanup() {
  kill "${PID[@]}" "${EXTRA_PIDS[@]}" 2>/dev/null
  rm -rf "$DIR"
}
EXTRA_PIDS=()
trap cleanup EXIT

spawn() { sleep 300 & PID[$1]=$!; }   # a fake session named $1
for s in A B C D; do spawn "$s"; done

N=0; FAILS=0
pass() { N=$((N + 1)); echo "  ok   $1"; }
fail() { N=$((N + 1)); FAILS=$((FAILS + 1)); echo "  FAIL $1"; }
check() { local desc="$1"; shift; if "$@"; then pass "$desc"; else fail "$desc"; fi; }

# lock_as NAME cmd... -> device-lock.sh as session NAME
lock_as() {
  local name="$1"; shift
  PICODROID_DEVICE_OWNER="$name" PICODROID_DEVICE_OWNER_PID="${PID[$name]}" "$DL" "$@"
}
field() { sed -n "s/^$1=//p" "$HOLDER" 2>/dev/null; }
is_free() { "$DL" status --quiet; }
is_held() { ! "$DL" status --quiet; }
queue_empty() { [[ -z "$(ls -A "$DIR/queue" 2>/dev/null)" ]]; }
rc_is() { local want="$1"; shift; "$@" >/dev/null 2>&1; [[ $? -eq $want ]]; }
# lib_req NAME args... -> require_device_lock through lib.sh as session NAME
lib_req() {
  local name="$1" script="$2"; shift 2
  PICODROID_DEVICE_OWNER="$name" PICODROID_DEVICE_OWNER_PID="${PID[$name]}" SCRIPT_DIR="$SCRIPT_DIR" \
    bash -c 'source "$SCRIPT_DIR/lib.sh"; require_device_lock "$@"' "$script" "$@"
}

echo "device-lock tests ($DIR)"

# 1. fresh
check "fresh dir is free" is_free

# 2. acquire / idempotent / busy
check "A acquires" rc_is 0 lock_as A acquire --note first
check "holder is A" [ "$(field owner)" == A ]
check "A re-acquires (idempotent)" rc_is 0 lock_as A acquire --note second
check "note refreshed" [ "$(field note)" == second ]
err="$(lock_as B acquire 2>&1 >/dev/null)"; rc=$?
check "B is refused with 75" [ $rc -eq 75 ]
check "refusal names A and the note" grep -q 'held by A (second)' <<<"$err"

# 3. FIFO hand-off: B, C, D queue while A holds; each takes, logs, releases.
order="$DIR/order"; : > "$order"
jobs=()
for w in B C D; do
  ( lock_as "$w" acquire --wait 20 >/dev/null 2>&1 \
      && echo "$w" >> "$order" \
      && lock_as "$w" release >/dev/null 2>&1 ) &
  jobs+=($!)
  sleep 0.3
done
sleep 0.5
st="$("$DL" status | tr -d '\n')"
check "status lists the queue B, C, D in order" grep -q '1) B.*2) C.*3) D' <<<"$st"
kill "${PID[A]}" 2>/dev/null; wait "${PID[A]}" 2>/dev/null
wait "${jobs[@]}"
check "hand-off order is B C D" [ "$(tr '\n' ' ' < "$order")" == "B C D " ]
check "board free after the chain" is_free
check "queue empty after the chain" queue_empty
spawn A

# 4. auto-release when the owner dies
lock_as A acquire >/dev/null
kill "${PID[A]}" 2>/dev/null; wait "${PID[A]}" 2>/dev/null
check "dead owner -> free" is_free
check "stale holder file swept" [ ! -f "$HOLDER" ]
check "B acquires after A died" rc_is 0 lock_as B acquire
lock_as B release >/dev/null
spawn A

# 5. pid reuse guard: live pid, wrong start time
printf 'owner=X\nlive_pid=%s\nlive_start=999\npinned=0\nnote=\nsince=0\ncwd=\nbranch=\n' "${PID[C]}" > "$HOLDER"
check "live pid with wrong start time is stale" is_free

# 6. run: lease tied to the run process
lock_as A run -- sleep 1 >/dev/null 2>&1 &
rj=$!
sleep 0.4
check "run holds while the command runs" is_held
check "run's lease is owned by A" [ "$(field owner)" == A ]
wait "$rj"
check "run releases afterwards" is_free
check "run propagates the exit code" rc_is 1 lock_as A run -- false
check "run releases after a failing command" is_free
lock_as A acquire >/dev/null
check "run nests inside an existing lease" rc_is 0 lock_as A run -- true
check "nested run keeps the lease" is_held
lock_as A release >/dev/null

# 7. break
lock_as A acquire >/dev/null
check "break without --force refuses" rc_is 1 "$DL" break
check "holder unchanged after refusal" [ "$(field owner)" == A ]
out="$("$DL" break --force 2>&1)"; rc=$?
check "break --force evicts" [ $rc -eq 0 ]
check "eviction names A" grep -q 'held by A' <<<"$out"
check "free after eviction" is_free

# 8. wrong-owner release
lock_as A acquire >/dev/null
check "B cannot release A's lease" rc_is 1 lock_as B release
check "holder still A" [ "$(field owner)" == A ]
lock_as A release >/dev/null

# 9. pinned lease
check "--pin without an owner fails" rc_is 1 "$DL" acquire --pin
check "pinned acquire with owner" rc_is 0 env PICODROID_DEVICE_OWNER=soak "$DL" acquire --pin --note soak
check "pinned=1" [ "$(field pinned)" == 1 ]
check "pinned lease has no live pid" [ -z "$(field live_pid)" ]
start=$(date +%s)
check "waiter times out on a pinned lease" rc_is 75 lock_as B acquire --wait 1
check "timed-out waiter left no ticket" queue_empty
check "timeout took ~1 s" [ $(( $(date +%s) - start )) -le 3 ]
check "pinned owner releases" rc_is 0 env PICODROID_DEVICE_OWNER=soak "$DL" release
check "free after pinned release" is_free

# 10. ghost tickets are pruned
mkdir -p "$DIR/queue"
printf 'ghost\n0\n' > "$DIR/queue/00000001-2147483000"
printf 'stale\n%s\n' "$(sed 's/^.*) //' "/proc/${PID[C]}/stat" | awk '{print $20}')" > "$DIR/queue/00000002-${PID[C]}"
touch -d '-60 seconds' "$DIR/queue/00000002-${PID[C]}"
check "ghost tickets do not block a no-wait acquire" rc_is 0 lock_as B acquire
check "ghost tickets pruned" queue_empty
lock_as B release >/dev/null

# 11. garbage holder files
: > "$HOLDER"
check "empty holder file is stale" rc_is 0 lock_as B acquire
lock_as B release >/dev/null
echo "note=orphan" > "$HOLDER"
check "holder without owner= is stale" rc_is 0 lock_as B acquire
lock_as B release >/dev/null

# 12. note is data, not code
lock_as A acquire --note '$(touch '"$DIR"'/pwned)' >/dev/null
check "note shown literally" grep -qF '$(touch' <<<"$("$DL" status)"
check "note not executed" [ ! -e "$DIR/pwned" ]
lock_as A release >/dev/null

# 13. --wait timeout removes its ticket
lock_as A acquire >/dev/null
check "waiter gives up with 75" rc_is 75 lock_as B acquire --wait 1
check "no ticket left behind" queue_empty

# 14/15. lib.sh::require_device_lock (A still holds)
check "require_device_lock refuses B with 75" rc_is 75 lib_req B flash.sh
check "PICODROID_DEVICE_LOCK=0 bypasses" rc_is 0 env PICODROID_DEVICE_LOCK=0 SCRIPT_DIR="$SCRIPT_DIR" bash -c \
  'source "$SCRIPT_DIR/lib.sh"; require_device_lock' flash.sh
check "require_device_lock is idempotent for A" rc_is 0 lib_req A flash.sh --app x
check "lib note is the script name + args" [ "$(field note)" == "flash.sh --app x" ]
lock_as A release >/dev/null
check "require_device_lock auto-acquires a free board" rc_is 0 lib_req B pdb.sh ping
check "auto-acquired lease is B's" [ "$(field owner)" == B ]
lock_as B release >/dev/null

# 16. opt-in: release kills a lingering probe-rs (never run beside a real one)
if [[ "${PICODROID_DEVICE_LOCK_TEST_PROBE:-0}" == "1" ]] && ! pgrep -x probe-rs >/dev/null; then
  cp "$(command -v sleep)" "$DIR/probe-rs"
  "$DIR/probe-rs" 300 & fp=$!; EXTRA_PIDS+=("$fp")
  lock_as A acquire >/dev/null
  env -u PICODROID_DEVICE_LOCK_KEEP_PROBE PICODROID_DEVICE_OWNER=A PICODROID_DEVICE_OWNER_PID="${PID[A]}" \
    "$DL" release --keep-probe >/dev/null
  sleep 0.2
  check "release --keep-probe leaves probe-rs alive" kill -0 "$fp"
  lock_as A acquire >/dev/null
  env -u PICODROID_DEVICE_LOCK_KEEP_PROBE PICODROID_DEVICE_OWNER=A PICODROID_DEVICE_OWNER_PID="${PID[A]}" \
    "$DL" release >/dev/null
  sleep 0.2
  check "release kills the lingering probe-rs" bash -c "! kill -0 $fp 2>/dev/null"
else
  echo "  skip probe-kill case (set PICODROID_DEVICE_LOCK_TEST_PROBE=1 with no real probe-rs running)"
fi

echo "device-lock tests: $((N - FAILS))/$N passed"
[[ $FAILS -eq 0 ]]
