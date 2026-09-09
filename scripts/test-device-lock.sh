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
# Set-but-empty = no fleet: the legacy sections below must see one board
# even on a bench that has ~/.config/picodroid/fleet.conf (fleet-lib.sh).
export PICODROID_FLEET_CONF=""
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

# 17. fleet-lib.sh: config parsing and slot resolution (no hardware; the
# fake config points at USB positions that do not exist).
FLEET="$DIR/fleet.conf"
cat > "$FLEET" <<'FEOF'
# two slots
a|SER_A|1-99.1|testbench_rp2350|probe_path=1-99.2
b|SER_B|1-99.3|testbench_rp2040,testbench_rp2350w
FEOF
# fl cmd... -> a fleet-lib.sh function with the fake config active
fl() { PICODROID_FLEET_CONF="$FLEET" bash -c 'source "$1/fleet-lib.sh"; shift; "$@"' _ "$SCRIPT_DIR" "$@"; }
check "no fleet when PICODROID_FLEET_CONF is empty" bash -c 'source "$1/fleet-lib.sh"; ! fleet_enabled' _ "$SCRIPT_DIR"
check "fleet enabled with the fake config" fl fleet_enabled
check "slots in config order" [ "$(fl fleet_slots | tr '\n' ' ')" == "a b " ]
check "probe serial field" [ "$(fl fleet_slot_field b probe_serial)" == SER_B ]
check "usb path field" [ "$(fl fleet_slot_field a board_usb_path)" == 1-99.1 ]
check "unknown slot -> rc 1" rc_is 1 fl fleet_slot_field zzz boards
check "extra value" [ "$(fl fleet_slot_extra a probe_path)" == 1-99.2 ]
check "missing extra -> empty" [ -z "$(fl fleet_slot_extra b probe_path)" ]
check "primary board is the first listed" [ "$(fl fleet_slot_primary b)" == testbench_rp2040 ]
check "slot has its second board" fl fleet_slot_has_board b testbench_rp2350w
check "slot lacks a foreign board" rc_is 1 fl fleet_slot_has_board a testbench_rp2040
check "board -> slot" [ "$(fl fleet_slot_for_board testbench_rp2350w)" == b ]
check "unknown board -> rc 1" rc_is 1 fl fleet_slot_for_board pico_enviro_mon
check "serial -> slot" [ "$(fl fleet_slot_for_serial SER_A)" == a ]
check "usb path -> slot" [ "$(fl fleet_slot_for_usb_path 1-99.3)" == b ]
check "fleet_check_conf accepts the fake config" fl fleet_check_conf
check "resolve --slot" [ "$(fl fleet_resolve_slot '' --slot b)" == b ]
check "resolve --board" [ "$(fl fleet_resolve_slot '' --app x --board testbench_rp2350w)" == b ]
check "resolve -b" [ "$(fl fleet_resolve_slot '' -b testbench_rp2350 --app x)" == a ]
check "resolve --board=" [ "$(fl fleet_resolve_slot '' --board=testbench_rp2040)" == b ]
check "resolve --boards picks the first that maps" [ "$(fl fleet_resolve_slot '' --hil --boards pico_enviro_mon,testbench_rp2350)" == a ]
check "resolve from PICODROID_SLOT" [ "$(PICODROID_SLOT=b fl fleet_resolve_slot '')" == b ]
check "resolve from PICODROID_BOARD" [ "$(PICODROID_BOARD=testbench_rp2350 fl fleet_resolve_slot '')" == a ]
check "resolve from the single held slot" [ "$(fl fleet_resolve_slot b ping)" == b ]
check "two held slots is ambiguous" rc_is 1 fl fleet_resolve_slot "$(printf 'a\nb')" ping
check "two slots and no hint is ambiguous" rc_is 1 fl fleet_resolve_slot ''
amb_out="$(fl fleet_resolve_slot '' 2>&1 || true)"
check "ambiguity lists the slots" grep -q 'testbench_rp2040' <<<"$amb_out"
check "unknown --slot -> rc 1" rc_is 1 fl fleet_resolve_slot '' --slot zzz
check "unknown --board -> rc 1" rc_is 1 fl fleet_resolve_slot '' --board pico_enviro_mon
check "explicit --slot beats a held slot" [ "$(fl fleet_resolve_slot b --slot a)" == a ]
check "usb_hub_port on a hub port" [ "$(fl usb_hub_port 1-8.3.2)" == "1-8.3 2" ]
check "usb_hub_port on a root port" [ "$(fl usb_hub_port 1-8)" == "1 8" ]
check "usb_hub_port rejects garbage" rc_is 1 fl usb_hub_port ttyACM0
check "unknown serial -> rc 1" rc_is 1 fl usb_path_for_serial NO_SUCH_SERIAL

# 18. fleet-lib.sh: the USB enumeration-storm guard, against a fake sysfs.
# A FIFO with no writer blocks its reader the way a hub's string attribute
# does while the kernel holds that hub's lock; a plain file answers at once.
USBFS="$DIR/usbfs"
mkdir -p "$USBFS/1-1" "$USBFS/1-2"
echo "Quiet Corp" > "$USBFS/1-1/manufacturer"
fu() { PICODROID_USB_SYSFS="$USBFS" fl "$@"; }
check "quiet sysfs -> nothing blocked, rc 0" [ -z "$(fu usb_sysfs_blocked)" ]
check "quiet sysfs -> wait_usb_quiet says nothing" [ -z "$(fu wait_usb_quiet 5)" ]
mkfifo "$USBFS/1-2/manufacturer"
check "a blocked device is named" [ "$(fu usb_sysfs_blocked | tr -d ' ')" == "$USBFS/1-2" ]
check "a blocked device -> rc 1" rc_is 1 fu usb_sysfs_blocked
storm_out="$(fu wait_usb_quiet 4 2>&1 || true)"
check "wait_usb_quiet reports the storm" grep -q 'enumeration storm on' <<<"$storm_out"
check "wait_usb_quiet gives up after its budget" grep -q 'still blocked' <<<"$storm_out"
check "wait_usb_quiet -> rc 1 when it gave up" rc_is 1 fu wait_usb_quiet 4
# release the readers parked on the FIFO
timeout 2 sh -c "echo x > '$USBFS/1-2/manufacturer'" 2>/dev/null || true
rm -f "$USBFS/1-2/manufacturer"
check "unknown usb path has no tty" rc_is 1 fl usb_path_tty 1-99.1
check "probe selector falls back to the Debug Probe ids" [ "$(fl probe_selector SER_A)" == 2e8a:000c:SER_A ]
fl_export() {
  PICODROID_FLEET_CONF="$FLEET" bash -c 'source "$1/fleet-lib.sh"; fleet_export_slot b
    [[ $PICODROID_SLOT == b && $PICODROID_PROBE_SERIAL == SER_B \
       && $PICODROID_BOARD_USB_PATH == 1-99.3 && $PROBE_RS_PROBE == 2e8a:000c:SER_B ]]' _ "$SCRIPT_DIR"
}
check "export sets the slot variables" fl_export
printf 'a|SER_A|1-99.1|testbench_rp2350\na|SER_C|1-99.4|nosuchboard|bogus=1\n' > "$DIR/bad.conf"
bad_out="$(PICODROID_FLEET_CONF="$DIR/bad.conf" bash -c 'source "$1/fleet-lib.sh"; fleet_check_conf' _ "$SCRIPT_DIR" 2>&1)"; bad_rc=$?
check "fleet_check_conf rejects a bad config" [ $bad_rc -eq 1 ]
check "  ... names the duplicate slot" grep -q "listed twice" <<<"$bad_out"
check "  ... names the unknown board" grep -q "nosuchboard" <<<"$bad_out"
check "  ... names the unknown extra" grep -q "bogus=1" <<<"$bad_out"
check "single-slot config resolves with no hint" [ "$(printf 'only|S|1-99.9|testbench_rp2350\n' > "$DIR/one.conf"; PICODROID_FLEET_CONF="$DIR/one.conf" bash -c 'source "$1/fleet-lib.sh"; fleet_resolve_slot ""' _ "$SCRIPT_DIR")" == only ]

# 18. one lease per slot (fleet mode: the fake config from section 17)
# flock_as NAME cmd... -> device-lock.sh as session NAME with the fleet on
flock_as() { PICODROID_FLEET_CONF="$FLEET" lock_as "$@"; }
fdl() { PICODROID_FLEET_CONF="$FLEET" "$DL" "$@"; }
HA="$DIR/slots/a/holder"; HB="$DIR/slots/b/holder"
sfield() { sed -n "s/^$2=//p" "$1" 2>/dev/null; }
check "--slot without a fleet is refused" rc_is 1 lock_as A acquire --slot a
check "A acquires slot a" rc_is 0 flock_as A acquire --slot a --note one
check "B acquires slot b by board" rc_is 0 flock_as B acquire --board testbench_rp2350w
check "slot a holder is A" [ "$(sfield "$HA" owner)" == A ]
check "slot b holder is B" [ "$(sfield "$HB" owner)" == B ]
check "legacy holder untouched" [ ! -f "$HOLDER" ]
err="$(flock_as C acquire --board testbench_rp2040 2>&1 >/dev/null)"; rc=$?
check "C is refused slot b with 75" [ $rc -eq 75 ]
check "refusal names the slot and B" grep -q 'device lock \[b\]: busy -- held by B' <<<"$err"
check "A cannot take slot b" rc_is 75 flock_as A acquire --slot b
check "A re-acquires a with no hint (single held slot)" rc_is 0 flock_as A acquire --note two
check "note refreshed on slot a" [ "$(sfield "$HA" note)" == two ]
check "C with no hint is refused (two slots, none held)" rc_is 1 flock_as C acquire
check "unknown slot -> rc 1" rc_is 1 flock_as C acquire --slot zzz
check "unknown board -> rc 1" rc_is 1 flock_as C acquire --board pico_enviro_mon
check "mine lists A's slot" [ "$(flock_as A mine)" == a ]
check "mine is empty for C" [ -z "$(flock_as C mine)" ]
st="$(fdl status | tr -d '\n')"
check "status lists both slots" grep -q 'device lock \[a\].*HELD by A.*device lock \[b\].*HELD by B' <<<"$st"
check "status --quiet is 1 while any slot is held" rc_is 1 fdl status --quiet
check "status --slot a --quiet is 1" rc_is 1 fdl status --slot a --quiet
check "status --short names the slot" grep -q '^b: held by B' <<<"$(fdl status --slot b --short)"
check "B releases with no slot (everything B holds)" rc_is 0 flock_as B release
check "slot b free, slot a still held" bash -c "[ ! -f '$HB' ] && [ -f '$HA' ]"
check "status --slot b --quiet is 0" rc_is 0 fdl status --slot b --quiet
check "B cannot release slot a" rc_is 1 flock_as B release --slot a
check "run --slot b holds b for the command" rc_is 0 flock_as B run --slot b -- bash -c "[ -f '$HB' ]"
check "run exports the slot to the command" rc_is 0 flock_as B run --board testbench_rp2040 -- bash -c '[[ $PICODROID_SLOT == b && $PROBE_RS_PROBE == 2e8a:000c:SER_B ]]'
check "run released b afterwards" [ ! -f "$HB" ]
check "break needs a slot with two configured" rc_is 1 fdl break --force
check "break --slot a evicts A" rc_is 0 fdl break --slot a --force
check "all free after the eviction" rc_is 0 fdl status --quiet
check "release with nothing held is fine" rc_is 0 flock_as A release
# A legacy single-board lease (a session on a branch without the fleet
# code) blocks every slot until it goes away.
lock_as C acquire --note old-branch >/dev/null
check "legacy lease refuses a slot acquire with 75" rc_is 75 flock_as A acquire --slot a
err="$(flock_as A acquire --slot a 2>&1 >/dev/null || true)"
check "  ... and says the whole bench is held" grep -q 'whole bench is held by C' <<<"$err"
check "  ... status --quiet is 1" rc_is 1 fdl status --quiet
check "  ... status names the legacy lease" grep -q 'legacy single-board lease' <<<"$(fdl status)"
check "  ... a waiter times out on it" rc_is 75 flock_as A acquire --slot a --wait 1
lock_as C release >/dev/null
check "slot acquire works once the legacy lease is gone" rc_is 0 flock_as A acquire --slot a
flock_as A release >/dev/null

# 19. scoped probe kill: release only kills the probe-rs on this slot's
# probe. Safe beside a real probe-rs: the fakes carry serials no real
# probe has, and only matching processes are touched.
cp "$(command -v sleep)" "$DIR/probe-rs"
PROBE_RS_PROBE=2e8a:000c:SER_A "$DIR/probe-rs" 300 & fa=$!; EXTRA_PIDS+=("$fa")
PROBE_RS_PROBE=2e8a:000c:SER_B "$DIR/probe-rs" 300 & fb=$!; EXTRA_PIDS+=("$fb")
sleep 0.2
flock_as A acquire --slot a >/dev/null
flock_as A acquire --slot b >/dev/null
out="$(env -u PICODROID_DEVICE_LOCK_KEEP_PROBE PICODROID_FLEET_CONF="$FLEET" \
  PICODROID_DEVICE_OWNER=A PICODROID_DEVICE_OWNER_PID="${PID[A]}" "$DL" release --slot a 2>&1)"
sleep 0.2
check "release --slot a kills the SER_A probe-rs" bash -c "! kill -0 $fa 2>/dev/null"
check "  ... and says so" grep -q "killed lingering probe-rs" <<<"$out"
check "  ... but leaves the SER_B probe-rs alive" kill -0 "$fb"
env -u PICODROID_DEVICE_LOCK_KEEP_PROBE PICODROID_FLEET_CONF="$FLEET" \
  PICODROID_DEVICE_OWNER=A PICODROID_DEVICE_OWNER_PID="${PID[A]}" "$DL" release --slot b >/dev/null 2>&1
sleep 0.2
check "release --slot b kills the SER_B probe-rs" bash -c "! kill -0 $fb 2>/dev/null"

# 20. lib.sh::require_device_lock picks the slot (fleet mode)
# flib_req NAME script args... -> require_device_lock through lib.sh, fleet on
flib_req() { PICODROID_FLEET_CONF="$FLEET" lib_req "$@"; }
# flib_env NAME args... -> the variables require_device_lock exports, as "SLOT PROBE"
flib_env() {
  local name="$1"; shift
  PICODROID_FLEET_CONF="$FLEET" PICODROID_DEVICE_OWNER="$name" PICODROID_DEVICE_OWNER_PID="${PID[$name]}" \
    SCRIPT_DIR="$SCRIPT_DIR" bash -c 'source "$SCRIPT_DIR/lib.sh"; require_device_lock "$@" >/dev/null
      echo "$PICODROID_SLOT $PROBE_RS_PROBE $PICODROID_BOARD_USB_PATH"' flash.sh "$@"
}
check "--board takes the matching slot" rc_is 0 flib_req A flash.sh --board testbench_rp2040 --app x
check "  ... slot b is A's" [ "$(sfield "$HB" owner)" == A ]
check "  ... slot a untouched" [ ! -f "$HA" ]
check "  ... note is the script name + args" [ "$(sfield "$HB" note)" == "flash.sh --board testbench_rp2040 --app x" ]
check "exports slot, probe selector and usb path" [ "$(flib_env A -b testbench_rp2040)" == "b 2e8a:000c:SER_B 1-99.3" ]
check "no hint reuses A's held slot" [ "$(flib_env A ping)" == "b 2e8a:000c:SER_B 1-99.3" ]
check "C with no hint and two slots is refused" rc_is 1 flib_req C pdb.sh ping
check "C is refused slot b with 75" rc_is 75 flib_req C pdb.sh --board testbench_rp2350w ping
check "C takes slot a via --boards" rc_is 0 flib_req C parity-bench.sh --hil --boards pico_enviro_mon,testbench_rp2350
check "  ... slot a is C's" [ "$(sfield "$HA" owner)" == C ]
check "unknown board is refused with 1" rc_is 1 flib_req A flash.sh --board pico_enviro_mon
check "PICODROID_DEVICE_LOCK=0 still exports the slot" [ "$(PICODROID_DEVICE_LOCK=0 flib_env A --slot a 2>/dev/null)" == "a 2e8a:000c:SER_A 1-99.1" ]
check "  ... without taking it" [ "$(sfield "$HA" owner)" == C ]
flock_as A release >/dev/null; flock_as C release >/dev/null
check "everything free again" rc_is 0 fdl status --quiet

echo "device-lock tests: $((N - FAILS))/$N passed"
[[ $FAILS -eq 0 ]]
