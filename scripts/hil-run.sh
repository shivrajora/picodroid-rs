#!/usr/bin/env bash
# Hardware-in-the-loop test runner for picodroid.
#
# Flashes each example app to an RP2350 device, captures RTT (defmt) output
# via probe-rs, and verifies expected log patterns.
#
# Usage:
#   ./scripts/hil-run.sh                  # run all tests, send email report
#   ./scripts/hil-run.sh --app helloworld # run one test only
#   ./scripts/hil-run.sh --board testbench_rp2040 --app langsuite_kt --no-email
#   ./scripts/hil-run.sh --no-email       # skip email report
#   ./scripts/hil-run.sh --include-hw     # also run hardware-peripheral tests
#   ./scripts/hil-run.sh --app netdemo --no-email   # one `net` row (needs .wifi-creds.env)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

# ── Configuration ────────────────────────────────────────────────────────────

HIL_CONF="$SCRIPT_DIR/hil-tests.conf"
HIL_DIR="$REPO_ROOT/build/hil"
# How long the nightly queues for the board before recording a SKIP.
HIL_LOCK_WAIT="${HIL_LOCK_WAIT:-3600}"
# Where PAPKs are built. hil-fleet.sh gives every slot its own directory so
# parallel runners never share a package file (and its own CARGO_TARGET_DIR,
# which resolve_board turns into TARGET_DIR, for the firmware).
HIL_APK_DIR="${HIL_APK_DIR:-$REPO_ROOT/build/apks}"

BOARD=""
SLOT=""
PULL=true

INCLUDE_HW=false
SKIP_PDB=false
SPECIFIC_APP=""
SEND_EMAIL=true
# Shrink matrix: every test runs once with shrinking off (default runtime
# behavior) and once with it on. Override with --mode to run a single side.
MODES=("no-shrink" "shrink")

# ── Argument parsing ────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --include-hw) INCLUDE_HW=true; shift ;;
    --skip-pdb)   SKIP_PDB=true; shift ;;
    --no-email)   SEND_EMAIL=false; shift ;;
    --app)        SPECIFIC_APP="$2"; shift 2 ;;
    --board)      BOARD="$2"; shift 2 ;;
    --slot)       SLOT="$2"; shift 2 ;;
    --no-pull)    PULL=false; shift ;;
    --mode)
      case "$2" in
        no-shrink) MODES=("no-shrink") ;;
        shrink)    MODES=("shrink") ;;
        both)      MODES=("no-shrink" "shrink") ;;
        *) echo "Unknown --mode value: $2 (want no-shrink|shrink|both)" >&2; exit 1 ;;
      esac
      shift 2
      ;;
    -h|--help)
      cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --app <name>    Run only the specified test
  --board <name>  Board to flash (default: testbench_rp2350; e.g. testbench_rp2040
                  when that board is on the probe). With a fleet config the
                  board picks the bench slot that runs it.
  --slot <name>   Bench slot to use (fleet config, scripts/fleet-lib.sh); the
                  board defaults to the slot's first listed board
  --no-pull       Skip the git pull (hil-fleet.sh pulls once for every slot)
  --include-hw    Also run hardware-peripheral tests (adcdemo, i2cdemo, etc.)
  --skip-pdb      Skip all PDB (Picodroid Debug Bridge) tests
  --mode <no-shrink|shrink|both>
                  Shrink modes to exercise (default: both). Every selected
                  test runs once per mode to catch regressions on either side.
  --no-email      Skip sending the email report
  -h, --help      Show this help message

net rows (netdemo, http_get) build the row's own W-board firmware with the
WiFi credentials from the gitignored .wifi-creds.env at the repo root
(PICODROID_WIFI_SSID=... / PICODROID_WIFI_PASS=...), point the app at this
machine's LAN IP, and run an echo server (7000) and an HTTP server (8000)
here for the duration. Without the creds file, or when the row's MCU is not
the one on the probe, they are SKIPped with a reason.
EOF
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

# Board and slot. With a fleet config (scripts/fleet-lib.sh) --slot names the
# bench slot and defaults the board to its first listed one, --board picks
# the slot that runs that board, and neither means the historical default.
# Without a fleet the board is simply the one on the probe. Results and logs
# are kept per slot, so hil-email's new-vs-known triage compares like with
# like and two runners never share a file.
if fleet_enabled; then
  if [[ -n "$SLOT" ]]; then
    if ! fleet_slot_row "$SLOT" >/dev/null; then
      echo "ERROR: unknown slot '$SLOT'" >&2; fleet_list_slots >&2; exit 1
    fi
    if [[ -z "$BOARD" ]]; then
      BOARD="$(fleet_slot_primary "$SLOT")"
    elif ! fleet_slot_has_board "$SLOT" "$BOARD"; then
      echo "ERROR: slot $SLOT does not run board $BOARD" >&2; fleet_list_slots >&2; exit 1
    fi
  else
    [[ -n "$BOARD" ]] || BOARD="testbench_rp2350"
    if ! SLOT="$(fleet_slot_for_board "$BOARD")"; then
      echo "ERROR: no slot in the fleet runs board $BOARD" >&2; fleet_list_slots >&2; exit 1
    fi
  fi
else
  if [[ -n "$SLOT" ]]; then
    echo "ERROR: --slot needs a fleet config (${PICODROID_FLEET_CONF-$FLEET_CONF_DEFAULT})" >&2; exit 1
  fi
  [[ -n "$BOARD" ]] || BOARD="testbench_rp2350"
fi
HIL_LOG_DIR="$HIL_DIR/logs${SLOT:+/$SLOT}"
HIL_RESULTS_DIR="$HIL_DIR/results${SLOT:+/$SLOT}"
LOCK_SLOT_ARGS=()
[[ -n "$SLOT" ]] && LOCK_SLOT_ARGS=(--slot "$SLOT")

resolve_board "$BOARD"

if [[ "$PLATFORM" == "esp" ]]; then
  echo "ERROR: HIL tests are not yet supported for ESP boards." >&2
  exit 1
fi

if [[ -z "$PROBE_CHIP" ]]; then
  echo "ERROR: no probe-rs chip mapping for this board's MCU (see resolve_board in lib.sh)." >&2
  exit 1
fi

# The command-line board is the one physically on the probe. `net` rows
# switch resolve_board to their own (W) board for one row and come back to
# this one afterwards; a row whose MCU differs from DEFAULT_MCU is skipped
# rather than flashed onto the wrong chip.
DEFAULT_BOARD="$BOARD"
DEFAULT_MCU="$MCU"

# `net` row prerequisites, checked once. Only presence is logged, never values.
NET_CREDS_FILE="$REPO_ROOT/.wifi-creds.env"
HAVE_NET_CREDS=false
if [[ -f "$NET_CREDS_FILE" ]] \
   && grep -qE '^PICODROID_WIFI_SSID=.+' "$NET_CREDS_FILE" 2>/dev/null; then
  HAVE_NET_CREDS=true
fi
NET_TEST_HOST="$(host_lan_ip || true)"

# ── Helpers ──────────────────────────────────────────────────────────────────

hil_log() { timestamp_log "$@"; }

PROBE_POLL_INTERVAL=1
PROBE_POLL_TIMEOUT=15

# Fleet: only this slot's probe and board ports (lib.sh::power_cycle_bench),
# so the other boards keep running. Legacy: every port of the probe's hub.
power_cycle_all() {
  hil_log "Power-cycling${SLOT:+ slot $SLOT}..."
  if ! power_cycle_bench 2>&1 | \
       while IFS= read -r line; do hil_log "  uhubctl: $line"; done; then
    hil_log "  WARNING: power cycle failed, continuing"
  fi
  sleep 5
  wait_for_probe
}

# Every probe-rs launch waits for the host's USB sysfs to answer first
# (fleet-lib.sh::wait_usb_quiet): a probe-rs started into an enumeration
# storm blocks in its device listing until the kernel gives up, and a row
# launched that way times out on an empty log.
usb_quiet() {
  wait_usb_quiet 2>&1 | while IFS= read -r line; do hil_log "  $line"; done || true
}

# Poll until the debug probe is detected by probe-rs.
wait_for_probe() {
  local elapsed=0
  usb_quiet
  while [[ $elapsed -lt $PROBE_POLL_TIMEOUT ]]; do
    if probe-rs list 2>/dev/null | grep -q "${PICODROID_PROBE_SERIAL:-CMSIS-DAP}"; then
      hil_log "  Probe detected after ${elapsed}s"
      pin_debug_probe >/dev/null
      return
    fi
    sleep "$PROBE_POLL_INTERVAL"
    elapsed=$((elapsed + PROBE_POLL_INTERVAL))
  done
  hil_log "  WARNING: Probe not detected within ${PROBE_POLL_TIMEOUT}s"
}

# Fleet: only the probe-rs on this slot's probe (fleet-lib.sh::probe_rs_pids
# matches the process name and its PROBE_RS_PROBE), so a sibling runner's
# attach survives. Legacy: every probe-rs of this uid, by name -- never
# `pkill -f probe-rs`, which matched any shell mentioning the word.
kill_probe_rs() { kill_probe_rs_scoped "${PICODROID_PROBE_SERIAL:-}" >/dev/null || true; }

# hil_pdb_run TIMEOUT args...: one pdb command against this slot's board.
# Fleet: the board's tty is looked up from its USB position at every call
# (the ttyACM number moves after a power cycle) and passed as -s, so the
# tool never scans the host and lands on a neighbour. A board that is not
# enumerated prints the tool's own "no picodroid devices found" line, which
# the callers already turn into a SKIP.
PDB_ENUM_WAIT=20
hil_pdb_run() {
  local t="$1"; shift
  local -a port=()
  if [[ -n "${PICODROID_BOARD_USB_PATH:-}" ]]; then
    # The board re-enumerates a few seconds after a reset or a power cycle
    # (7 s seen on the bench); give it PDB_ENUM_WAIT s before calling it
    # absent, and want the /dev node as well as the sysfs entry.
    local p="" waited=0
    until p=$(usb_path_tty "$PICODROID_BOARD_USB_PATH") && [[ -e "$p" ]]; do
      if (( waited >= PDB_ENUM_WAIT )); then
        echo "error: no picodroid devices found (slot $SLOT: usb $PICODROID_BOARD_USB_PATH not enumerated after ${PDB_ENUM_WAIT}s)"
        return 1
      fi
      sleep 1
      waited=$((waited + 1))
    done
    port=(-s "$p")
  fi
  # -k: a pdb blocked in a kernel call on a vanishing tty ignores TERM.
  timeout -k 5 "$t" "$PDB_BIN" ${port[@]+"${port[@]}"} "$@"
}

# row_matches_board LIST: a term/loop/hw row's board filter (comma list of
# board or MCU names) names the board on this probe or its MCU.
row_matches_board() {
  local need
  for need in ${1//,/ }; do
    if [[ "$need" == "$DEFAULT_BOARD" || "$need" == "$DEFAULT_MCU" ]]; then return 0; fi
  done
  return 1
}

recover_probe() {
  hil_log "Recovering probe..."
  kill_probe_rs
  sleep 1
  power_cycle_all
}

# Kill an entire process group started with setsid.
# Args: pid
kill_process_group() {
  local pid="$1"
  kill -- -"$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

# Build + install a known-good helloworld PAPK via pdb. Called before each
# install-reject-* test so a previously-flashed broken app can't brick the
# bench and cascade-fail every downstream rejection test. Returns 0 on
# success, 1 otherwise (build failure or device unreachable).
#
# Args: mode ("no-shrink"|"shrink"), log_prefix (used for build + install logs)
pdb_install_known_good() {
  local mode="$1" log_prefix="$2"
  local apk_path="$HIL_APK_DIR/helloworld.papk"
  # --strip-debug on every PAPK built here: they all go to a release-profile
  # firmware, built without the line-numbers feature, which never reads
  # LineNumberTable/SourceFile (build-apk.sh --help). Its (pc=N) frames
  # resolve on the host through scripts/retrace.sh.
  local -a apk_args=(--app helloworld --board "$BOARD" --strip-debug -o "$apk_path")
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "${log_prefix}.known-good-build.log" 2>&1; then
    hil_log "  Known-good helloworld PAPK build failed"
    return 1
  fi
  if ! hil_pdb_run 60 install "$apk_path" > "${log_prefix}.known-good-install.log" 2>&1; then
    hil_log "  Known-good pdb install failed (see ${log_prefix}.known-good-install.log)"
    return 1
  fi
  sleep 3  # let the device reboot into fresh helloworld and re-enumerate CDC
  return 0
}

run_pdb_test() {
  local app="$1" timeout="$2" patterns="$3" pdb_cmd="$4" mode="$5"
  local test_name="$app:pdb-$pdb_cmd[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-${pdb_cmd}.${mode}.log"

  TOTAL=$((TOTAL + 1))
  hil_log "--- [$TOTAL] $test_name (pdb, ${timeout}s) ---"

  # After an RTT test, probe-rs may leave the MCU halted. Reset the device
  # so it boots normally and the USB CDC port enumerates.
  hil_log "  Resetting device..."
  usb_quiet
  probe-rs reset --chip "$PROBE_CHIP" --protocol swd </dev/null 2>/dev/null || true
  sleep 3  # wait for USB CDC enumeration

  # Early SKIP for install-reject-future when not in shrink mode — do this
  # before the bench-reset pre-install so we don't waste a build cycle.
  if [[ "$pdb_cmd" == "install-reject-future" && "$mode" != "shrink" ]]; then
    hil_log "SKIP $test_name (only meaningful in shrink mode)"
    echo "SKIP $test_name" >> "$RESULTS_FILE"
    SKIP=$((SKIP + 1))
    return
  fi

  # The package-directory rows need multi-app firmware: SKIP when the
  # greeting carries no apps tail (a single-app board on the probe), and the
  # fixture-minting rows also need papk-pack.
  case "$pdb_cmd" in
    list|uninstall|install-reject-noroom|install-compact|launch|launch-soak|settings-uninstall)
      local pre_ping="$RUN_LOG_DIR/${app}.pdb-${pdb_cmd}.${mode}.pre-ping.log"
      if ! hil_pdb_run 10 ping > "$pre_ping" 2>&1 \
          || ! grep -qE 'apps [0-9]+/[0-9]+' "$pre_ping"; then
        hil_log "SKIP $test_name (single-app firmware: no package directory)"
        echo "SKIP $test_name (single-app firmware)" >> "$RESULTS_FILE"
        SKIP=$((SKIP + 1))
        return
      fi
      if [[ -z "$PAPK_PACK_BIN" && "$pdb_cmd" == install-* ]]; then
        hil_log "SKIP $test_name (papk-pack unavailable for fixtures)"
        echo "SKIP $test_name (no papk-pack)" >> "$RESULTS_FILE"
        SKIP=$((SKIP + 1))
        return
      fi
      ;;
  esac

  # Cascade guard for install-reject-* tests: pre-install a known-good
  # helloworld so a prior broken app can't brick the bench and make every
  # downstream rejection test fail for bench-state reasons rather than the
  # rejection behavior under test. Seen in the wild when gcstress's on-boot
  # OOM left the CDC port dead for all subsequent pdb tests. The package
  # rows start from the same known state.
  case "$pdb_cmd" in
    install-reject-*|list|uninstall|install-compact)
      hil_log "  Pre-installing known-good helloworld to restore bench state..."
      if ! pdb_install_known_good "$mode" "$RUN_LOG_DIR/${app}.pdb-${pdb_cmd}-${mode}"; then
        hil_log "  SKIP ($test_name): bench unresponsive — cannot validate rejection"
        echo "SKIP $test_name (bench unresponsive)" >> "$RESULTS_FILE"
        SKIP=$((SKIP + 1))
        return
      fi
      ;;
  esac

  local -a pdb_args=()
  case "$pdb_cmd" in
    ping)
      pdb_args=(ping)
      ;;
    sysmon)
      pdb_args=(sysmon)
      ;;
    install)
      # The installed PAPK must match whatever mode the currently-flashed
      # firmware was built in, or verify_compat rejects the install.
      local apk_path="$HIL_APK_DIR/${app}.papk"
      local -a apk_args=(--app "$app" --board "$BOARD" --strip-debug -o "$apk_path")
      [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
      hil_log "  Building PAPK ($mode)..."
      if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$RUN_LOG_DIR/${app}.pdb-${mode}.build.log" 2>&1; then
        hil_log "  BUILD FAILED (PAPK for install)"
        echo "ERROR $test_name (papk build failed)" >> "$RESULTS_FILE"
        ERROR=$((ERROR + 1))
        return
      fi
      pdb_args=(install "$apk_path")
      ;;
    install-reject-host|install-reject-device)
      # Build the PAPK in the OPPOSITE mode of the running firmware so its
      # framework-map-version is incompatible. We expect pdb to refuse the
      # install (host pre-flight or device-side STATUS_INCOMPAT) and the
      # device to remain alive afterwards.
      local opp_mode="no-shrink"
      [[ "$mode" == "no-shrink" ]] && opp_mode="shrink"
      local apk_path="$HIL_APK_DIR/${app}.papk"
      local -a apk_args=(--app "$app" --board "$BOARD" --strip-debug -o "$apk_path")
      [[ "$opp_mode" == "shrink" ]] && apk_args+=(--shrink)
      hil_log "  Building opposite-mode PAPK ($opp_mode) for $pdb_cmd..."
      if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$RUN_LOG_DIR/${app}.pdb-${pdb_cmd}-${mode}.build.log" 2>&1; then
        hil_log "  BUILD FAILED (PAPK for $pdb_cmd)"
        echo "ERROR $test_name (papk build failed)" >> "$RESULTS_FILE"
        ERROR=$((ERROR + 1))
        return
      fi
      pdb_args=(install --expect-rejected)
      [[ "$pdb_cmd" == "install-reject-device" ]] && pdb_args+=(--skip-host-check)
      pdb_args+=("$apk_path")
      ;;
    install-reject-future)
      # Synthesize a higher version map and build a PAPK against it so its
      # framework-map-version is "from the future" relative to firmware.
      # Only meaningful in shrink mode (no-shrink can't trigger it; both
      # sides would be 0.0.0, the symmetric-accept case) — that early SKIP
      # happens before the bench-reset pre-install above.
      local apk_path="$HIL_APK_DIR/${app}.papk"
      hil_log "  Building future-version PAPK..."
      # Its own lock, not the gradle one (the script builds a PAPK under
      # that): it mints a synthetic map in the shared sdk/shrink-maps/ and
      # removes it on exit, so two runners must take turns.
      if ! flock -w 900 "$REPO_ROOT/build/.hil-future-map.lock" \
           bash "$SCRIPT_DIR/test-future-version-rejection.sh" "$app" "$apk_path" \
            > "$RUN_LOG_DIR/${app}.pdb-${pdb_cmd}-${mode}.build.log" 2>&1; then
        hil_log "  BUILD FAILED (future PAPK)"
        echo "ERROR $test_name (future papk build failed)" >> "$RESULTS_FILE"
        ERROR=$((ERROR + 1))
        return
      fi
      pdb_args=(install --expect-rejected "$apk_path")
      ;;
    install-reject-truncated)
      # Build a valid PAPK, then truncate it to a stub so the manifest magic
      # and framework-map-version are gone. Covers the realistic corruption
      # case (partial download, damaged SD card, etc.) that install-reject-*
      # version-mismatch rows don't exercise. Expect refusal + device alive.
      # Its own path: this fixture is truncated in place, and the shared
      # build/apks/<app>.papk is what test.sh and sim.sh reuse afterwards.
      local apk_path="$HIL_APK_DIR/hil/${app}-truncated.papk"
      local -a apk_args=(--app "$app" --board "$BOARD" --strip-debug -o "$apk_path")
      [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
      hil_log "  Building PAPK for $pdb_cmd ($mode)..."
      if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$RUN_LOG_DIR/${app}.pdb-${pdb_cmd}-${mode}.build.log" 2>&1; then
        hil_log "  BUILD FAILED (PAPK for $pdb_cmd)"
        echo "ERROR $test_name (papk build failed)" >> "$RESULTS_FILE"
        ERROR=$((ERROR + 1))
        return
      fi
      hil_log "  Truncating PAPK to 100 bytes to corrupt manifest..."
      truncate -s 100 "$apk_path"
      pdb_args=(install --expect-rejected "$apk_path")
      ;;
    install-stress)
      run_pdb_install_stress "$app" "$timeout" "$patterns" "$mode"
      return
      ;;
    list)
      pdb_args=(list)
      ;;
    uninstall)
      run_pdb_uninstall_test "$app" "$patterns" "$mode"
      return
      ;;
    install-reject-noroom)
      run_pdb_noroom_test "$app" "$patterns" "$mode"
      return
      ;;
    install-compact)
      run_pdb_compact_test "$app" "$patterns" "$mode"
      return
      ;;
    launch)
      run_pdb_launch_test "$app" "$patterns" "$mode" 0
      return
      ;;
    launch-soak)
      run_pdb_launch_test "$app" "$patterns" "$mode" 20
      return
      ;;
    settings-uninstall)
      run_pdb_settings_test "$app" "$patterns" "$mode"
      return
      ;;
    *)
      hil_log "  ERROR: unknown PDB command '$pdb_cmd'"
      echo "ERROR $test_name (unknown pdb command)" >> "$RESULTS_FILE"
      ERROR=$((ERROR + 1))
      return
      ;;
  esac

  # Run PDB tool with timeout; capture stdout and stderr.
  hil_log "  Running: pdb ${pdb_args[*]}"
  local exit_code=0
  hil_pdb_run "$timeout" "${pdb_args[@]}" > "$log_file" 2>&1 || exit_code=$?

  if [[ $exit_code -ne 0 ]]; then
    # "no picodroid devices found" → graceful SKIP.
    if grep -q "no picodroid devices found" "$log_file" 2>/dev/null; then
      hil_log "  SKIP (no CDC device detected)"
      echo "SKIP $test_name" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      return
    fi
    if [[ $exit_code -eq 124 ]]; then
      hil_log "  PDB command timed out after ${timeout}s"
    else
      hil_log "  PDB exited with code $exit_code"
    fi
  fi

  # For install-reject-* tests we also assert the device is still alive
  # afterwards: a clean rejection must not have erased flash or rebooted.
  local reject_test=false
  case "$pdb_cmd" in
    install-reject-*) reject_test=true ;;
  esac
  if $reject_test; then
    sleep 1
    local ping_log="${log_file%.log}.post-ping.log"
    if ! hil_pdb_run 5 ping > "$ping_log" 2>&1 \
        || ! grep -q "max PAPK" "$ping_log"; then
      hil_log "  FAIL (device unresponsive after rejection — flash may have been erased)"
      tail -5 "$ping_log" 2>/dev/null \
        | while IFS= read -r line; do hil_log "    post-ping: $line"; done || true
      echo "FAIL $test_name (post-ping liveness)" >> "$RESULTS_FILE"
      FAIL=$((FAIL + 1))
      return
    fi
  fi

  # Check expected patterns.
  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1; then
    hil_log "  PASS"
    echo "PASS $test_name" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    hil_log "  FAIL"
    hil_log "  Log tail:"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do hil_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do hil_log "  $line"; done || true
    echo "FAIL $test_name" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}

# ── Package-directory rows (multi-app firmware) ─────────────────────────────

# Repack the known-good helloworld PAPK (built by pdb_install_known_good in
# the row's mode) under another package name, padded to `bytes`.
hil_repack() {
  local package="$1" bytes="$2" out="$3"
  "$PAPK_PACK_BIN" --repack "$HIL_APK_DIR/helloworld.papk" \
    --package-name "$package" --label "$package" --pad-asset "$bytes" \
    --output "$out" > /dev/null 2>&1
}

# One pdb command, appended to the row's log. Generous per-command timeout:
# an install may compact the region (up to ~30 s) and every install or
# uninstall waits for the reboot.
hil_pdb() {
  local log="$1"
  shift
  hil_pdb_run 300 "$@" >> "$log" 2>&1
}

# Build the release firmware for the bench board: `apk_path` baked in as the
# boot-default app, the system apps (the launcher) linked in
# (lib.sh::build_system_apks), and `boot` as the boot override — empty for
# the ordinary rows, `launcher` for the launch rows. Any further arguments
# are env entries for cargo (the net rows' credentials). Appends to `log`.
hil_build_firmware() {
  local apk_path="$1" mode="$2" boot="$3" log="$4"
  shift 4
  local -a cargo_env=(PICODROID_APK_PATH="$apk_path")
  [[ "$mode" == "shrink" ]] && cargo_env+=(PICODROID_SHRINK=1)
  cargo_env+=("$@")
  # The system apps (the launcher, settings) are built in the row's mode;
  # PICODROID_SHRINK is put back afterwards so a later no-shrink row's PAPKs
  # are not shrunk by accident. Both variables build_system_apks exports are
  # set even when empty: build.rs reruns when either changes.
  # They go to this slot's APK directory (two runners in different shrink
  # modes must not share one file) and reach build_system_apks as prebuilts,
  # which then only exports the variables.
  if [[ "${MAX_INSTALLED_APPS:-1}" -gt 1 ]]; then
    local system_app_dir system_app prebuilt=""
    for system_app_dir in "$REPO_ROOT"/system-apps/*/; do
      [[ -f "$system_app_dir/PicodroidManifest.xml" ]] || continue
      system_app="$(basename "$system_app_dir")"
      local -a system_args=(--app "$system_app" --board "$BOARD" --strip-debug -o "$HIL_APK_DIR/$system_app.papk")
      [[ "$mode" == "shrink" ]] && system_args+=(--shrink)
      bash "$SCRIPT_DIR/build-apk.sh" "${system_args[@]}" >> "$log" 2>&1 || return 1
      prebuilt="${prebuilt:+$prebuilt:}$HIL_APK_DIR/$system_app.papk"
    done
    export PICODROID_PREBUILT_SYSTEM_APKS="$prebuilt"
  fi
  export PICODROID_BOOT="$boot"
  build_system_apks >> "$log" 2>&1 || return 1
  env "${cargo_env[@]}" cargo build \
    -p picodroid \
    --release \
    --jobs "${HIL_JOBS:-$(cpu_count)}" \
    --target "$TARGET" \
    --no-default-features \
    --features "$BOARD_FEATURE" >> "$log" 2>&1
}

# Flash `elf` with probe-rs run (which also streams RTT), wait for the
# package directory's boot line, then release the probe. The RTT capture
# is appended to `log`. Returns 1 when probe-rs reported an error or the
# device never booted.
hil_flash_elf() {
  local elf="$1" log="$2"
  local flash_log="${log%.log}.flash.log"
  kill_probe_rs
  sleep 1
  usb_quiet
  setsid timeout 120 \
    probe-rs run --chip "$PROBE_CHIP" --protocol swd "$elf" \
    < /dev/null > "$flash_log" 2>&1 &
  local pid=$! elapsed=0
  while kill -0 "$pid" 2>/dev/null && [[ $elapsed -lt 100 ]]; do
    sleep 1
    elapsed=$((elapsed + 1))
    if grep -q "packages\] boot:" "$flash_log" 2>/dev/null; then break; fi
  done
  kill_process_group "$pid"
  cat "$flash_log" >> "$log"
  if grep -qE "^Error: " "$flash_log"; then return 1; fi
  grep -q "packages\] boot:" "$flash_log"
}

# Attach RTT to the running device (no flash, no reset) so a pdb-driven row
# can also record what the launcher and the app log. Prints the pid; the
# caller ends it with kill_process_group and appends its log.
hil_rtt_attach() {
  local elf="$1" log="$2"
  setsid probe-rs attach --chip "$PROBE_CHIP" --protocol swd "$elf" \
    < /dev/null > "$log" 2>&1 &
  echo $!
}

# The free heap `pdb sysmon` reports, in bytes; the whole report goes to `log`.
hil_free_heap() {
  local log="$1" out
  out=$(hil_pdb_run 30 sysmon 2>&1 < /dev/null) || true
  printf '%s\n' "$out" >> "$log"
  sed -nE 's/^Free heap: +([0-9]+) bytes.*/\1/p' <<< "$out" | head -1
}

# launch: flash `app` with the launcher as the boot app (--boot launcher).
# The launcher runs and `pdb list` names it; tap row 0 — `app` is the only
# installed app, so that is its row — and `pdb list` names `app`. Touch
# boards only: `pdb input tap` needs a touch panel, so the row SKIPs where
# the device refuses it.
#
# launch-soak (`cycles` > 0): the same with an app that exits at once
# (helloworld), tapped `cycles` more times. Every tap runs the app and
# returns to the launcher — two `run_app` re-entries per cycle, the path an
# install park used to be the only user of — and the free heap `pdb sysmon`
# reports before and after must agree within 4 KB.
run_pdb_launch_test() {
  local app="$1" patterns="$2" mode="$3" cycles="${4:-0}"
  local kind=launch
  [[ "$cycles" -gt 0 ]] && kind=launch-soak
  local test_name="$app:pdb-$kind[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-$kind.${mode}.log"
  local build_log="$RUN_LOG_DIR/${app}.pdb-$kind.${mode}.build.log"
  : > "$log_file"

  hil_log "  Building $app firmware with --boot launcher ($mode)..."
  local apk_path="$HIL_APK_DIR/${app}.papk"
  local -a apk_args=(--app "$app" --board "$BOARD" --strip-debug -o "$apk_path")
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1 \
     || ! hil_build_firmware "$apk_path" "$mode" launcher "$build_log"; then
    hil_log "  BUILD FAILED (see $build_log)"
    echo "ERROR $test_name (build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi
  local elf="$TARGET_DIR/${TARGET}/release/picodroid"
  hil_log "  Flashing..."
  if ! hil_flash_elf "$elf" "$log_file"; then
    hil_log "  FLASH FAILED (see log tail)"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do hil_log "    $line"; done || true
    echo "ERROR $test_name (flash failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    recover_probe
    return
  fi
  usb_quiet
  probe-rs reset --chip "$PROBE_CHIP" --protocol swd </dev/null 2>/dev/null || true
  sleep 4

  local rtt_log="${log_file%.log}.rtt.log" rtt_pid
  rtt_pid=$(hil_rtt_attach "$elf" "$rtt_log")
  sleep 2

  # Row 0 is `app` only when nothing else is installed (rows sort by
  # label): erase what earlier rows left behind. Each uninstall reboots the
  # device, and the launcher rebuilds its list at every boot.
  echo "=== clearing other packages ===" >> "$log_file"
  local other
  for other in $(hil_pdb_run 30 list 2>/dev/null < /dev/null \
      | awk '$1 ~ /^[0-9]+$/ { print $2 }'); do
    [[ "$other" == "$app" ]] && continue
    hil_pdb "$log_file" uninstall "$other" || true
  done
  sleep 2
  echo "=== boot ===" >> "$log_file"
  hil_pdb "$log_file" list || true
  local tap_log="${log_file%.log}.tap.log"
  if ! hil_pdb_run 30 input tap 120 20 > "$tap_log" 2>&1 < /dev/null; then
    cat "$tap_log" >> "$log_file"
    if grep -qi "no touch panel" "$tap_log"; then
      kill_process_group "$rtt_pid"
      hil_log "SKIP $test_name (no touch panel: the tap cannot reach the launcher)"
      echo "SKIP $test_name (no touch panel)" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      return
    fi
  fi
  if [[ "$cycles" -eq 0 ]]; then
    sleep 3
    echo "=== after the tap ===" >> "$log_file"
    hil_pdb "$log_file" list || true
  else
    sleep 3
    local before after i home=0
    before=$(hil_free_heap "$log_file")
    for i in $(seq 1 "$cycles"); do
      hil_pdb_run 30 input tap 120 20 >> "$log_file" 2>&1 < /dev/null || true
      sleep 3
      if hil_pdb_run 30 list 2>/dev/null < /dev/null | grep -q "running: picodroid.launcher"; then
        home=$((home + 1))
      fi
    done
    after=$(hil_free_heap "$log_file")
    echo "soak: $home/$cycles cycles returned to the launcher" >> "$log_file"
    echo "free heap: before ${before:-?} after ${after:-?}" >> "$log_file"
    if [[ -n "$before" && -n "$after" && $((before - after)) -le 4096 && $((after - before)) -le 4096 ]]; then
      echo "heap drift ok" >> "$log_file"
    fi
    hil_pdb "$log_file" list || true
  fi
  kill_process_group "$rtt_pid"
  echo "=== rtt ===" >> "$log_file"
  cat "$rtt_log" >> "$log_file" 2>/dev/null || true
  local launched
  launched=$(grep -c "Launcher: launch" "$rtt_log" 2>/dev/null || true)
  echo "rtt saw ${launched:-0} launch line(s)" >> "$log_file"
  hil_pdb_verdict "$test_name" "$log_file" "$patterns"
}

# settings-uninstall: flash the app with `--boot launcher`, open Settings
# (the launcher's row 1: installed apps sort first), Apps, tap the app's
# row and the dialog's Uninstall; the app must be gone from the list and
# the settings app still running.
run_pdb_settings_test() {
  local app="$1" patterns="$2" mode="$3"
  local dialog_ok="${PICODROID_SETTINGS_DIALOG_OK:-$(settings_dialog_ok "$BOARD")}"
  local test_name="$app:pdb-settings-uninstall[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-settings-uninstall.${mode}.log"
  local build_log="$RUN_LOG_DIR/${app}.pdb-settings-uninstall.${mode}.build.log"
  : > "$log_file"

  hil_log "  Building $app firmware with --boot launcher ($mode)..."
  local apk_path="$HIL_APK_DIR/${app}.papk"
  local -a apk_args=(--app "$app" --board "$BOARD" --strip-debug -o "$apk_path")
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1 \
     || ! hil_build_firmware "$apk_path" "$mode" launcher "$build_log"; then
    hil_log "  BUILD FAILED (see $build_log)"
    echo "ERROR $test_name (build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi
  local elf="$TARGET_DIR/${TARGET}/release/picodroid"
  hil_log "  Flashing..."
  if ! hil_flash_elf "$elf" "$log_file"; then
    hil_log "  FLASH FAILED (see log tail)"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do hil_log "    $line"; done || true
    echo "ERROR $test_name (flash failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    recover_probe
    return
  fi
  usb_quiet
  probe-rs reset --chip "$PROBE_CHIP" --protocol swd </dev/null 2>/dev/null || true
  sleep 4

  local rtt_log="${log_file%.log}.rtt.log" rtt_pid
  rtt_pid=$(hil_rtt_attach "$elf" "$rtt_log")
  sleep 2

  # Only the app under test may be installed: it is row 0, Settings row 1.
  echo "=== clearing other packages ===" >> "$log_file"
  local other
  for other in $(hil_pdb_run 30 list 2>/dev/null < /dev/null \
      | awk '$1 ~ /^[0-9]+$/ { print $2 }'); do
    [[ "$other" == "$app" ]] && continue
    hil_pdb "$log_file" uninstall "$other" || true
  done
  sleep 2
  echo "=== boot ===" >> "$log_file"
  hil_pdb "$log_file" list || true
  local tap_log="${log_file%.log}.tap.log"
  if ! hil_pdb_run 30 input tap 120 60 > "$tap_log" 2>&1 < /dev/null; then
    cat "$tap_log" >> "$log_file"
    if grep -qi "no touch panel" "$tap_log"; then
      kill_process_group "$rtt_pid"
      hil_log "SKIP $test_name (no touch panel: the tap cannot reach the launcher)"
      echo "SKIP $test_name (no touch panel)" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      return
    fi
  fi
  sleep 3
  echo "=== in settings ===" >> "$log_file"
  hil_pdb "$log_file" list || true
  hil_pdb_run 30 input tap 120 100 >> "$log_file" 2>&1 < /dev/null || true   # Apps
  sleep 2
  hil_pdb_run 30 input tap 120 60 >> "$log_file" 2>&1 < /dev/null || true    # the app's row
  sleep 2
  # shellcheck disable=SC2086  # two coordinates
  hil_pdb_run 30 input tap $dialog_ok >> "$log_file" 2>&1 < /dev/null || true  # Uninstall
  sleep 4
  echo "=== after the uninstall ===" >> "$log_file"
  hil_pdb "$log_file" list || true
  kill_process_group "$rtt_pid"
  echo "=== rtt ===" >> "$log_file"
  cat "$rtt_log" >> "$log_file" 2>/dev/null || true
  hil_pdb_verdict "$test_name" "$log_file" "$patterns"
}

# PASS/FAIL a row from its log and patterns.
hil_pdb_verdict() {
  local test_name="$1" log_file="$2" patterns="$3"
  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1; then
    hil_log "  PASS"
    echo "PASS $test_name" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    hil_log "  FAIL"
    hil_log "  Log tail:"
    tail -8 "$log_file" 2>/dev/null | while IFS= read -r line; do hil_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do hil_log "  $line"; done || true
    echo "FAIL $test_name" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}

# uninstall: erase the known-good helloworld, then list — the directory must
# be empty and the device back.
run_pdb_uninstall_test() {
  local app="$1" patterns="$2" mode="$3"
  local test_name="$app:pdb-uninstall[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-uninstall.${mode}.log"
  : > "$log_file"
  hil_log "  Running: pdb uninstall helloworld; pdb list"
  hil_pdb "$log_file" uninstall helloworld || true
  hil_pdb "$log_file" list || true
  hil_pdb_verdict "$test_name" "$log_file" "$patterns"
}

# install-reject-noroom: fill the directory with repacked helloworlds, then
# one more must be refused with STATUS_NO_ROOM and nothing erased; the
# fillers are uninstalled afterwards so the bench is left as found.
run_pdb_noroom_test() {
  local app="$1" patterns="$2" mode="$3"
  local test_name="$app:pdb-install-reject-noroom[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-install-reject-noroom.${mode}.log"
  local fixtures="$RUN_LOG_DIR/fixtures"
  mkdir -p "$fixtures"
  : > "$log_file"

  local pre_ping="$RUN_LOG_DIR/${app}.pdb-install-reject-noroom.${mode}.pre-ping.log"
  hil_pdb_run 10 ping > "$pre_ping" 2>&1 || true
  local installed max
  installed=$(sed -nE 's/.*apps ([0-9]+)\/([0-9]+).*/\1/p' "$pre_ping" | head -1)
  max=$(sed -nE 's/.*apps ([0-9]+)\/([0-9]+).*/\2/p' "$pre_ping" | head -1)
  local fillers=$(( ${max:-0} - ${installed:-0} ))
  hil_log "  Directory ${installed:-?}/${max:-?}: installing $fillers fillers, then one too many"
  local i
  for i in $(seq 1 "$fillers"); do
    hil_repack "hilfill$i" 0 "$fixtures/hilfill$i.papk" || { echo "repack hilfill$i failed" >> "$log_file"; break; }
    echo "=== filler $i/$fillers ===" >> "$log_file"
    hil_pdb "$log_file" install "$fixtures/hilfill$i.papk" || echo "filler $i: install failed" >> "$log_file"
  done
  hil_repack hilfill_extra 0 "$fixtures/hilfill_extra.papk" || echo "repack extra failed" >> "$log_file"
  echo "=== one too many ===" >> "$log_file"
  hil_pdb "$log_file" install --expect-rejected "$fixtures/hilfill_extra.papk" || echo "extra: not rejected cleanly" >> "$log_file"
  hil_pdb "$log_file" list || true
  echo "=== cleanup ===" >> "$log_file"
  for i in $(seq 1 "$fillers"); do
    hil_pdb "$log_file" uninstall "hilfill$i" || true
  done
  hil_pdb_verdict "$test_name" "$log_file" "$patterns"
}

# install-compact: four ~300 KB apps, drop the first and third, then a
# ~500 KB one that only fits once the region is compacted. Its fixtures are
# uninstalled afterwards.
run_pdb_compact_test() {
  local app="$1" patterns="$2" mode="$3"
  local test_name="$app:pdb-install-compact[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-install-compact.${mode}.log"
  local fixtures="$RUN_LOG_DIR/fixtures"
  mkdir -p "$fixtures"
  : > "$log_file"
  local name
  for name in a b c d; do
    hil_repack "hilfill_$name" $((300 * 1024)) "$fixtures/hilfill_$name.papk" || { echo "repack $name failed" >> "$log_file"; }
    echo "=== install hilfill_$name (300 KB) ===" >> "$log_file"
    hil_pdb "$log_file" install "$fixtures/hilfill_$name.papk" || echo "hilfill_$name: install failed" >> "$log_file"
  done
  for name in a c; do
    echo "=== uninstall hilfill_$name ===" >> "$log_file"
    hil_pdb "$log_file" uninstall "hilfill_$name" || echo "hilfill_$name: uninstall failed" >> "$log_file"
  done
  hil_repack hilfill_e $((500 * 1024)) "$fixtures/hilfill_e.papk" || echo "repack e failed" >> "$log_file"
  echo "=== install hilfill_e (500 KB, needs compaction) ===" >> "$log_file"
  hil_pdb "$log_file" install "$fixtures/hilfill_e.papk" || echo "hilfill_e: install failed" >> "$log_file"
  hil_pdb "$log_file" list || true
  echo "=== cleanup ===" >> "$log_file"
  for name in b d e; do
    hil_pdb "$log_file" uninstall "hilfill_$name" || true
  done
  hil_pdb_verdict "$test_name" "$log_file" "$patterns"
}

run_pdb_install_stress() {
  local app="$1" timeout="$2" patterns="$3" mode="$4"
  local test_name="$app:pdb-install-stress[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-install-stress.${mode}.log"

  # Alternate between blinky and displaydemo for different PAPK sizes.
  local -a stress_apps=(blinky displaydemo)

  # Build PAPKs for both apps in the same mode as the flashed firmware.
  local -a sa_args
  for sa in "${stress_apps[@]}"; do
    local apk_path="$HIL_APK_DIR/${sa}.papk"
    hil_log "  Building PAPK for $sa ($mode)..."
    sa_args=(--app "$sa" --board "$BOARD" --strip-debug -o "$apk_path")
    [[ "$mode" == "shrink" ]] && sa_args+=(--shrink)
    if ! bash "$SCRIPT_DIR/build-apk.sh" "${sa_args[@]}" > "$RUN_LOG_DIR/${sa}.pdb-${mode}.build.log" 2>&1; then
      hil_log "  BUILD FAILED (PAPK for $sa)"
      echo "ERROR $test_name (papk build failed for $sa)" >> "$RESULTS_FILE"
      ERROR=$((ERROR + 1))
      return
    fi
  done

  local total_cycles=10
  local succeeded=0

  : > "$log_file"

  local deadline=$((SECONDS + timeout))

  for i in $(seq 1 $total_cycles); do
    if [[ $SECONDS -ge $deadline ]]; then
      echo "cycle $i/$total_cycles: TIMEOUT (overall deadline reached)" >> "$log_file"
      hil_log "  cycle $i/$total_cycles: TIMEOUT"
      continue
    fi

    # Alternate apps: odd=first, even=second.
    local idx=$(( (i - 1) % ${#stress_apps[@]} ))
    local sa="${stress_apps[$idx]}"
    local apk_path="$HIL_APK_DIR/${sa}.papk"
    local remaining=$((deadline - SECONDS))

    echo "=== cycle $i/$total_cycles: installing $sa ===" >> "$log_file"
    hil_log "  cycle $i/$total_cycles: installing $sa"

    if hil_pdb_run "$remaining" install "$apk_path" >> "$log_file" 2>&1; then
      succeeded=$((succeeded + 1))
      echo "cycle $i/$total_cycles: OK" >> "$log_file"
    else
      echo "cycle $i/$total_cycles: FAILED (exit $?)" >> "$log_file"
      hil_log "  cycle $i/$total_cycles: FAILED"
    fi
  done

  echo "$succeeded/$total_cycles install cycles succeeded" >> "$log_file"
  hil_log "  Result: $succeeded/$total_cycles install cycles succeeded"

  # Check expected patterns.
  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1; then
    hil_log "  PASS"
    echo "PASS $test_name" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    hil_log "  FAIL"
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do hil_log "  $line"; done || true
    echo "FAIL $test_name" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}



# ── Main ─────────────────────────────────────────────────────────────────────

mkdir -p "$HIL_LOG_DIR" "$HIL_RESULTS_DIR"

send_report() {
  [[ "$SEND_EMAIL" == "true" ]] || return 0
  hil_log "Sending email report..."
  python3 "$SCRIPT_DIR/hil-email.py" \
    --results "$RESULTS_FILE" \
    --log-dir "$HIL_LOG_DIR" \
    --run-id "$RUN_ID" \
    --sha "$COMMIT_SHA" \
    --suite "HIL${SLOT:+ $SLOT}" 2>&1 | while IFS= read -r line; do hil_log "  email: $line"; done || \
    hil_log "  Email sending failed (non-fatal)."
}

# Take the machine-wide board lease (scripts/device-lock.sh). Interactive
# sessions hold it across their flash/pdb work; the nightly queues behind
# them FIFO for up to HIL_LOCK_WAIT seconds and otherwise records a SKIP so
# the email says why nothing ran. The lease belongs to this process and is
# released on every exit path.
export PICODROID_DEVICE_OWNER="${PICODROID_DEVICE_OWNER:-hil-run${SLOT:+:$SLOT}}"
export PICODROID_DEVICE_OWNER_PID=$$
trap 'stop_net_listeners; bash "$SCRIPT_DIR/device-lock.sh" release ${LOCK_SLOT_ARGS[@]+"${LOCK_SLOT_ARGS[@]}"} >/dev/null 2>&1 || true' EXIT
hil_log "Waiting for the device lock${SLOT:+ on slot $SLOT} (up to ${HIL_LOCK_WAIT}s)..."
lock_rc=0
bash "$SCRIPT_DIR/device-lock.sh" acquire ${LOCK_SLOT_ARGS[@]+"${LOCK_SLOT_ARGS[@]}"} --wait "$HIL_LOCK_WAIT" \
  --note "nightly hil-run $BOARD $(date '+%Y-%m-%d %H:%M')" 2>&1 \
  | while IFS= read -r line; do hil_log "  lock: $line"; done || lock_rc=$?
if [[ $lock_rc -ne 0 ]]; then
  holder="$(bash "$SCRIPT_DIR/device-lock.sh" status ${LOCK_SLOT_ARGS[@]+"${LOCK_SLOT_ARGS[@]}"} --short)"
  COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
  RUN_ID="${HIL_RUN_ID:-$(date '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT_SHA}}"
  RESULTS_FILE="$HIL_RESULTS_DIR/${RUN_ID}.txt"
  hil_log "SKIPPED: device lock not acquired within ${HIL_LOCK_WAIT}s ($holder)"
  echo "SKIP hil-run (device busy: $holder)" > "$RESULTS_FILE"
  send_report
  exit 1
fi
hil_log "Device lock acquired."

# Pin probe-rs to the CMSIS-DAP probe before anything talks to it. With a
# second probe attached and no pin, probe-rs prompts on stdin and every row
# fails; with the pin it just works. Logged so the nightly email shows which
# probe the run used. Fleet: the slot decides (its probe serial, and the USB
# position of the board for pdb); otherwise an operator override via
# PROBE_RS_PROBE is kept as is.
if [[ -n "$SLOT" ]]; then
  fleet_export_slot "$SLOT"
  hil_log "Probe: slot $SLOT, serial $PICODROID_PROBE_SERIAL (usb $(usb_path_for_serial "$PICODROID_PROBE_SERIAL" || echo 'not enumerated')), PROBE_RS_PROBE=$PROBE_RS_PROBE"
  hil_log "Board USB: $PICODROID_BOARD_USB_PATH (tty $(usb_path_tty "$PICODROID_BOARD_USB_PATH" || echo 'not enumerated'))"
elif [[ -n "${PROBE_RS_PROBE:-}" ]]; then
  hil_log "Probe: PROBE_RS_PROBE=$PROBE_RS_PROBE (from environment)"
elif probe_pin="$(pin_debug_probe)" && [[ -n "$probe_pin" ]]; then
  hil_log "Probe: pinned PROBE_RS_PROBE=$probe_pin"
else
  hil_log "Probe: no CMSIS-DAP probe enumerated yet (power cycle + wait will pin it)"
fi

# The probe must actually attach, not just enumerate: a Debug Probe on
# firmware older than 2.2.0 is listed by probe-rs but refused by every
# command ("The firmware on the probe is outdated"), and without this check
# such a bench grinds through every row as an ERROR. Same SKIP shape as
# the busy-lock path so the email says why nothing ran.
# (`probe-rs info` exits 0 even then, so the verdict is its text -- and only
# the probe-level failures count: the RP2350's multidrop DP can report a
# per-port error on a sleeping target that the power cycle before the first
# row clears.)
probe_info="$(probe-rs info --protocol swd </dev/null 2>&1 || true)"
if grep -qiE 'Failed to open (the )?(debug )?probe|firmware on the probe|no probe was found|probe.*not found' <<<"$probe_info"; then
  probe_err="$(grep -iE 'Failed to open|firmware on the probe|no probe|not found' <<<"$probe_info" | tail -1 | sed 's/^ *//')"
  COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
  RUN_ID="${HIL_RUN_ID:-$(date '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT_SHA}}"
  RESULTS_FILE="$HIL_RESULTS_DIR/${RUN_ID}.txt"
  hil_log "SKIPPED: the probe does not attach${SLOT:+ (slot $SLOT)}: $probe_err"
  echo "SKIP hil-run (probe unusable: $probe_err)" > "$RESULTS_FILE"
  send_report
  exit 1
fi

# Pull latest code (hil-fleet.sh does this once, before its runners start).
if [[ "$PULL" == "true" ]]; then
  hil_log "Pulling latest code..."
  git -C "$REPO_ROOT" pull --ff-only 2>&1 | while IFS= read -r line; do hil_log "  git: $line"; done || true
fi

COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
RUN_ID="${HIL_RUN_ID:-$(date '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT_SHA}}"
RUN_LOG_DIR="$HIL_LOG_DIR/$RUN_ID"
RESULTS_FILE="$HIL_RESULTS_DIR/${RUN_ID}.txt"

mkdir -p "$RUN_LOG_DIR"

# Pre-build PDB host tool (needed for pdb category tests).
# hil-fleet.sh builds both host tools once and hands them over in
# HIL_PDB_BIN / HIL_PAPK_PACK_BIN; a runner on its own builds them into its
# own target directory (CARGO_TARGET_DIR when set).
PDB_HOST_TARGET="$(host_target)"
PDB_BIN="${HIL_PDB_BIN:-}"
if [[ "$SKIP_PDB" != "true" && -z "$PDB_BIN" ]]; then
  hil_log "Building PDB tool..."
  PDB_BIN="${CARGO_TARGET_DIR:-$REPO_ROOT/target}/${PDB_HOST_TARGET}/release/pdb"
  if ! cargo build --release --quiet \
      --target "$PDB_HOST_TARGET" \
      --manifest-path "$REPO_ROOT/tools/pdb/Cargo.toml" \
      > "$RUN_LOG_DIR/pdb-build.log" 2>&1; then
    hil_log "WARNING: PDB tool build failed; PDB tests will be skipped"
    PDB_BIN=""
  fi
fi

# Pin pdb to the bench board's CDC port on a bench without a fleet config.
# With a second picodroid board on the hub `pdb` refuses to pick one
# ("multiple picodroid devices found") and every pdb row fails. The bench
# board is the device whose greeting advertises this board's app region
# (max PAPK = app_region_kb - 4 KB): PDB_BIN becomes a wrapper that passes
# `-s <port>`. A PICODROID_PDB_PORT in the environment wins; with no match
# the auto-detect is left alone. A fleet slot pins per call instead
# (hil_pdb_run), so this does nothing when one is in use.
pin_pdb_port() {
  [[ -n "$PDB_BIN" ]] || return 0
  # A fleet slot names the board's USB position and hil_pdb_run passes its
  # tty on every call; this pin is for a bench without a fleet config.
  [[ -z "${PICODROID_BOARD_USB_PATH:-}" ]] || return 0
  local want=$(( ${APP_REGION_KB:-0} - 4 ))
  local port="${PICODROID_PDB_PORT:-}"
  if [[ -z "$port" ]]; then
    port=$(timeout 15 "$PDB_BIN" devices 2>/dev/null < /dev/null \
      | awk -v want="max PAPK: $want KB" 'index($0, want) { print $1; exit }')
  fi
  if [[ -z "$port" ]]; then
    hil_log "pdb: no device advertises a ${want} KB app region; port auto-detect stays"
    return 0
  fi
  local wrapper="$RUN_LOG_DIR/pdb-pinned"
  printf '#!/usr/bin/env bash\nexec %q -s %q "$@"\n' "$PDB_BIN" "$port" > "$wrapper"
  chmod +x "$wrapper"
  PDB_BIN="$wrapper"
  hil_log "pdb: pinned to $port (max PAPK ${want} KB)"
}
pin_pdb_port

# Pre-build papk-pack: the multi-app pdb rows mint their fixtures with
# `--repack` (same classes, another package name, padded to a size).
PAPK_PACK_BIN="${HIL_PAPK_PACK_BIN:-}"
if [[ -n "$PDB_BIN" && -z "$PAPK_PACK_BIN" ]]; then
  PAPK_PACK_BIN="${CARGO_TARGET_DIR:-$REPO_ROOT/target}/${PDB_HOST_TARGET}/release/papk-pack"
  if ! cargo build --release --quiet \
      --target "$PDB_HOST_TARGET" \
      --manifest-path "$REPO_ROOT/tools/papk-pack/Cargo.toml" \
      > "$RUN_LOG_DIR/papk-pack-build.log" 2>&1; then
    hil_log "WARNING: papk-pack build failed; the multi-app pdb rows will be skipped"
    PAPK_PACK_BIN=""
  fi
fi

hil_log "========================================="
hil_log "HIL Run: $RUN_ID"
hil_log "========================================="
hil_log "Board on probe: $DEFAULT_BOARD ($DEFAULT_MCU)${SLOT:+, slot $SLOT}"
hil_log "net rows: creds $([[ "$HAVE_NET_CREDS" == "true" ]] && echo present || echo MISSING) (.wifi-creds.env), test host ${NET_TEST_HOST:-NONE}"

PASS=0; FAIL=0; SKIP=0; ERROR=0; TOTAL=0

run_test() {
  local app="$1" category="$2" timeout="$3" patterns="$4" mode="$5" row_board="${6:-}"
  local tag="${app}[${mode}]"
  local log_file="$RUN_LOG_DIR/${app}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${app}.${mode}.build.log"

  TOTAL=$((TOTAL + 1))
  hil_log "--- [$TOTAL] $tag ($category, ${timeout}s${row_board:+, board $row_board}) ---"

  # Per-row board (net rows): resolve_board swaps BOARD_FEATURE/TARGET/... for
  # this row; the caller restores DEFAULT_BOARD afterwards.
  local board="$DEFAULT_BOARD"
  if [[ -n "$row_board" ]]; then
    resolve_board "$row_board"
    board="$row_board"
  fi

  # Power cycle devices to ensure clean state.
  power_cycle_all

  # Build APK (mode-tagged; --shrink iff this iteration is the shrunk one).
  hil_log "  Building APK..."
  local apk_path="$HIL_APK_DIR/${app}.papk"
  local -a apk_args=(--app "$app" --board "$board" --strip-debug -o "$apk_path")
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  # net rows: bake this machine's LAN IP into the app's NetTestConfig.HOST
  # (build-apk.sh forwards the env var as a per-invocation Gradle property).
  local -a apk_env=()
  [[ "$category" == "net" ]] && apk_env+=(PICODROID_NET_TEST_HOST="$NET_TEST_HOST")
  if ! env "${apk_env[@]}" bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1; then
    hil_log "  BUILD FAILED (APK)"
    echo "ERROR $tag (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi


  # Build firmware (release), the launcher linked in on a multi-app board.
  # PICODROID_SHRINK must match the APK's mode or verify_compat will reject
  # at load.
  hil_log "  Building firmware (release)..."
  # net rows: WiFi credentials are option_env! in the W firmware. Read straight
  # from the creds file into the env array; they never touch a log line.
  local -a extra_env=()
  if [[ "$category" == "net" ]]; then
    local cred
    while IFS= read -r cred; do
      extra_env+=("$cred")
    done < <(grep -E '^PICODROID_WIFI_(SSID|PASS|AUTH)=' "$NET_CREDS_FILE")
  fi
  if ! hil_build_firmware "$apk_path" "$mode" "" "$build_log" ${extra_env[@]+"${extra_env[@]}"}; then
    hil_log "  BUILD FAILED (firmware)"
    echo "ERROR $tag (firmware build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # Clean up any lingering probe-rs from previous test, then wait for probe.
  kill_probe_rs
  sleep 2

  # Flash the pre-built ELF and capture RTT output.
  #
  # `timeout` budget = test timeout (conf value, post-boot wall time the app
  # needs) + flash budget. probe-rs run does flash *and* RTT inside the same
  # invocation, so without the flash budget the per-test 30 s would be eaten
  # by the 22–27 s RP2350 flash and leave only a few seconds of RTT capture
  # — flake territory for `loop` apps whose first log line takes a moment to
  # appear (e.g. activities that bring up LVGL on the first onCreate).
  #
  # 35 s covers both the typical 22–27 s flash + a couple seconds of slop
  # without running so long that real hangs go unnoticed.
  local flash_budget=35
  # The conf's budgets are set against the RP2350 (150 MHz, 520 KB); the
  # RP2040 runs the same rows at roughly half the speed (benchmark: 33 s vs
  # 15 s for int_arithmetic on the 2026-09-09 bench), so its rows get twice
  # the post-boot window. Only the timeout scales -- never the patterns.
  if [[ "$DEFAULT_MCU" == "rp2040" ]]; then
    timeout=$((timeout * 2))
  fi
  local effective_timeout=$((timeout + flash_budget))
  local elf="$TARGET_DIR/${TARGET}/release/picodroid"
  usb_quiet
  hil_log "  Flashing and capturing RTT..."
  # stdin from /dev/null: probe-rs never gets to prompt (see pin_debug_probe),
  # and the config-file loop in main keeps its fd 3 to itself either way.
  setsid timeout "$effective_timeout" \
    probe-rs run --chip "$PROBE_CHIP" --protocol swd "$elf" \
    < /dev/null > "$log_file" 2>&1 &
  local run_pid=$!

  local result=1  # assume failure

  if [[ "$category" == "term" || "$category" == "hw" || "$category" == "net" ]]; then
    # Poll for expected output, kill early on match.
    local elapsed=0
    while kill -0 "$run_pid" 2>/dev/null && [[ $elapsed -lt $effective_timeout ]]; do
      sleep 1
      elapsed=$((elapsed + 1))
      if check_patterns "$log_file" "$patterns" > /dev/null 2>&1; then
        result=0
        break
      fi
    done
    kill_process_group "$run_pid"

  elif [[ "$category" == "loop" ]]; then
    # Let it run for the full effective timeout (flash + post-boot window),
    # then check patterns.
    wait "$run_pid" 2>/dev/null || true
    if check_patterns "$log_file" "$patterns" > /dev/null 2>&1; then
      result=0
    fi
  fi

  # probe-rs prints "Error: ..." when the probe is lost, flash verify fails,
  # or the MCU halts unexpectedly. If that marker is present the run result
  # is not trustworthy — report ERROR so operators know to inspect the bench
  # rather than chase a phantom content regression in the app.
  if grep -qE "^Error: " "$log_file" 2>/dev/null; then
    hil_log "  PROBE-RS ERROR (see log tail)"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do hil_log "    $line"; done || true
    echo "ERROR $tag (probe-rs error)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    recover_probe
    return
  fi

  # A passing pattern match is invalid if the log also contains a panic /
  # HardFault / CRASH marker — the app emitted its success token then faulted.
  if [[ $result -eq 0 ]] && ! check_no_crash "$log_file" > /dev/null 2>&1; then
    result=1
  fi

  # Evaluate result.
  if [[ $result -eq 0 ]]; then
    hil_log "  PASS"
    echo "PASS $tag" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    hil_log "  FAIL"
    hil_log "  Log tail:"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do hil_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do hil_log "  $line"; done || true
    check_no_crash "$log_file" 2>&1 | while IFS= read -r line; do hil_log "  $line"; done || true
    echo "FAIL $tag" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))

    # Try to recover for next test.
    recover_probe
  fi
}

# Run every selected test once per shrink mode. Each mode is a full pass
# through the config so pdb-install tests see firmware that matches their
# PAPK's mode.
for MODE in "${MODES[@]}"; do
  hil_log "========================================="
  hil_log "Mode: $MODE"
  hil_log "========================================="

  # The 5th column is the pdb command for pdb rows and the board for net rows.
  # The config is read on fd 3, not stdin: a child that reads stdin (an
  # interactive probe-rs prompt, a pdb call) must never be able to swallow
  # the rest of the config file -- that is how the 2026-09-05..07 nightlies
  # ran three rows, one of them an app named "r".
  while IFS='|' read -r -u 3 app category timeout patterns extra; do
    pdb_cmd="$extra"
    # Skip comments and blank lines.
    [[ "$app" =~ ^[[:space:]]*# ]] && continue
    [[ -z "$app" ]] && continue

    # If specific app requested, skip others.
    if [[ -n "$SPECIFIC_APP" && "$app" != "$SPECIFIC_APP" ]]; then
      continue
    fi

    # net rows: W-board firmware + creds + host listeners. Skip with a reason
    # when a prerequisite is missing so a checkout without bench creds, or a
    # different board on the probe, stays green instead of red.
    if [[ "$category" == "net" ]]; then
      net_skip=""
      if [[ "$HAVE_NET_CREDS" != "true" ]]; then
        net_skip="no .wifi-creds.env"
      elif [[ -z "$NET_TEST_HOST" ]]; then
        net_skip="no LAN IP"
      elif [[ -n "$SLOT" ]]; then
        # Fleet: the row runs on the slot that lists its board, nowhere else.
        if ! fleet_slot_has_board "$SLOT" "$extra"; then
          net_skip="board $extra is not on slot $SLOT"
        fi
      else
        resolve_board "$extra"
        row_mcu="$MCU"
        resolve_board "$DEFAULT_BOARD"
        if [[ "$row_mcu" != "$DEFAULT_MCU" ]]; then
          net_skip="row board $extra is $row_mcu, probe has $DEFAULT_MCU"
        fi
      fi
      if [[ -n "$net_skip" ]]; then
        hil_log "SKIP $app[$MODE] ($net_skip)"
        echo "SKIP $app[$MODE]" >> "$RESULTS_FILE"
        SKIP=$((SKIP + 1))
        continue
      fi
      if ! start_net_listeners "$RUN_LOG_DIR"; then
        hil_log "ERROR $app[$MODE] ($NET_LISTENER_ERR)"
        echo "ERROR $app[$MODE] (listeners)" >> "$RESULTS_FILE"
        ERROR=$((ERROR + 1))
        TOTAL=$((TOTAL + 1))
        continue
      fi
      run_test "$app" "$category" "$timeout" "$patterns" "$MODE" "$extra"
      resolve_board "$DEFAULT_BOARD"
      continue
    fi

    # Skip hw-dependent tests unless --include-hw.
    if [[ "$category" == "hw" && "$INCLUDE_HW" != "true" ]]; then
      hil_log "SKIP $app[$MODE] (hardware-dependent)"
      echo "SKIP $app[$MODE]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # Skip sim-only tests (the HIL board lacks what they need, e.g. a
    # network stack; sim-run.sh runs them).
    if [[ "$category" == "sim" ]]; then
      hil_log "SKIP $app[$MODE] (sim-only)"
      echo "SKIP $app[$MODE]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # Skip explicitly skipped tests.
    if [[ "$category" == "skip" ]]; then
      hil_log "SKIP $app[$MODE]"
      echo "SKIP $app[$MODE]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # PDB tests: run PDB command against already-running device.
    if [[ "$category" == "pdb" ]]; then
      if [[ "$SKIP_PDB" == "true" ]]; then
        hil_log "SKIP $app:pdb-$pdb_cmd[$MODE] (--skip-pdb)"
        echo "SKIP $app:pdb-$pdb_cmd[$MODE]" >> "$RESULTS_FILE"
        SKIP=$((SKIP + 1))
        TOTAL=$((TOTAL + 1))
        continue
      fi
      if [[ -z "$PDB_BIN" || ! -x "$PDB_BIN" ]]; then
        hil_log "SKIP $app:pdb-$pdb_cmd[$MODE] (PDB tool not available)"
        echo "SKIP $app:pdb-$pdb_cmd[$MODE]" >> "$RESULTS_FILE"
        SKIP=$((SKIP + 1))
        TOTAL=$((TOTAL + 1))
        continue
      fi
      run_pdb_test "$app" "$timeout" "$patterns" "$pdb_cmd" "$MODE"
      continue
    fi

    # term/loop/hw rows may name the boards or MCUs they need (5th column).
    if [[ -n "$extra" ]] && ! row_matches_board "$extra"; then
      hil_log "SKIP $app[$MODE] (needs $extra; this is $DEFAULT_BOARD/$DEFAULT_MCU)"
      echo "SKIP $app[$MODE]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    run_test "$app" "$category" "$timeout" "$patterns" "$MODE"
  done 3< "$HIL_CONF"
done

# Give the board back before the summary and email (the EXIT trap is the
# safety net for every other exit path). This also kills a lingering probe-rs.
stop_net_listeners
bash "$SCRIPT_DIR/device-lock.sh" release ${LOCK_SLOT_ARGS[@]+"${LOCK_SLOT_ARGS[@]}"} 2>&1 | while IFS= read -r line; do hil_log "  lock: $line"; done || true

# Summary.
hil_log "========================================="
hil_log "HIL Run $RUN_ID Complete"
hil_log "  PASS: $PASS  FAIL: $FAIL  SKIP: $SKIP  ERROR: $ERROR"
hil_log "  Results: $RESULTS_FILE"
hil_log "  Logs:    $RUN_LOG_DIR/"
hil_log "========================================="

send_report

# Exit with failure if any tests failed.
[[ $FAIL -eq 0 && $ERROR -eq 0 ]]
