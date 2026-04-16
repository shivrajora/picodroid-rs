#!/usr/bin/env bash
# Hardware-in-the-loop test runner for picodroid.
#
# Flashes each example app to an RP2350 device, captures RTT (defmt) output
# via probe-rs, and verifies expected log patterns.
#
# Usage:
#   ./scripts/hil-run.sh                  # run all tests, send email report
#   ./scripts/hil-run.sh --app helloworld # run one test only
#   ./scripts/hil-run.sh --no-email       # skip email report
#   ./scripts/hil-run.sh --include-hw     # also run hardware-peripheral tests
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

# ── Configuration ────────────────────────────────────────────────────────────

HIL_CONF="$SCRIPT_DIR/hil-tests.conf"
HIL_DIR="$REPO_ROOT/build/hil"
HIL_LOG_DIR="$HIL_DIR/logs"
HIL_RESULTS_DIR="$HIL_DIR/results"
HIL_LOCK="$HIL_DIR/hil.lock"

BOARD="testbench_rp2350"

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
  --include-hw    Also run hardware-peripheral tests (adcdemo, i2cdemo, etc.)
  --skip-pdb      Skip all PDB (Picodroid Debug Bridge) tests
  --mode <no-shrink|shrink|both>
                  Shrink modes to exercise (default: both). Every selected
                  test runs once per mode to catch regressions on either side.
  --no-email      Skip sending the email report
  -h, --help      Show this help message
EOF
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

resolve_board "$BOARD"

# ── Helpers ──────────────────────────────────────────────────────────────────

hil_log() { timestamp_log "$@"; }

PROBE_POLL_INTERVAL=1
PROBE_POLL_TIMEOUT=15

power_cycle_all() {
  local hub
  hub=$(detect_usb_hub)
  if [[ -z "$hub" ]]; then
    hil_log "WARNING: No USB hub with CMSIS-DAP probe detected, skipping power cycle"
    return
  fi
  hil_log "Power-cycling all ports on hub $hub..."
  if ! sudo uhubctl -l "$hub" -a cycle 2>&1 | \
       while IFS= read -r line; do hil_log "  uhubctl: $line"; done; then
    hil_log "  WARNING: uhubctl returned non-zero exit status"
  fi
  sleep 5
  wait_for_probe
}

# Poll until the debug probe is detected by probe-rs.
wait_for_probe() {
  local elapsed=0
  while [[ $elapsed -lt $PROBE_POLL_TIMEOUT ]]; do
    if probe-rs list 2>/dev/null | grep -q "CMSIS-DAP"; then
      hil_log "  Probe detected after ${elapsed}s"
      return
    fi
    sleep "$PROBE_POLL_INTERVAL"
    elapsed=$((elapsed + PROBE_POLL_INTERVAL))
  done
  hil_log "  WARNING: Probe not detected within ${PROBE_POLL_TIMEOUT}s"
}

recover_probe() {
  hil_log "Recovering probe..."
  pkill -f "probe-rs" 2>/dev/null || true
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

run_pdb_test() {
  local app="$1" timeout="$2" patterns="$3" pdb_cmd="$4" mode="$5"
  local test_name="$app:pdb-$pdb_cmd[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-${pdb_cmd}.${mode}.log"

  TOTAL=$((TOTAL + 1))
  hil_log "--- [$TOTAL] $test_name (pdb, ${timeout}s) ---"

  # After an RTT test, probe-rs may leave the MCU halted. Reset the device
  # so it boots normally and the USB CDC port enumerates.
  hil_log "  Resetting device..."
  probe-rs reset --chip RP235x --protocol swd 2>/dev/null || true
  sleep 3  # wait for USB CDC enumeration

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
      local apk_path="$REPO_ROOT/build/apks/${app}.papk"
      local -a apk_args=(--app "$app")
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
      local apk_path="$REPO_ROOT/build/apks/${app}.papk"
      local -a apk_args=(--app "$app")
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
      # sides would be 0.0.0 which is the symmetric-accept case).
      if [[ "$mode" != "shrink" ]]; then
        hil_log "SKIP $test_name (only meaningful in shrink mode)"
        echo "SKIP $test_name" >> "$RESULTS_FILE"
        SKIP=$((SKIP + 1))
        return
      fi
      local apk_path="$REPO_ROOT/build/apks/${app}.papk"
      hil_log "  Building future-version PAPK..."
      if ! bash "$SCRIPT_DIR/test-future-version-rejection.sh" "$app" "$apk_path" \
            > "$RUN_LOG_DIR/${app}.pdb-${pdb_cmd}-${mode}.build.log" 2>&1; then
        hil_log "  BUILD FAILED (future PAPK)"
        echo "ERROR $test_name (future papk build failed)" >> "$RESULTS_FILE"
        ERROR=$((ERROR + 1))
        return
      fi
      pdb_args=(install --expect-rejected "$apk_path")
      ;;
    install-stress)
      run_pdb_install_stress "$app" "$timeout" "$patterns" "$mode"
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
  timeout "$timeout" "$PDB_BIN" "${pdb_args[@]}" > "$log_file" 2>&1 || exit_code=$?

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
    if ! timeout 5 "$PDB_BIN" ping > "$ping_log" 2>&1 \
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

run_pdb_install_stress() {
  local app="$1" timeout="$2" patterns="$3" mode="$4"
  local test_name="$app:pdb-install-stress[$mode]"
  local log_file="$RUN_LOG_DIR/${app}.pdb-install-stress.${mode}.log"

  # Alternate between blinky and displaydemo for different PAPK sizes.
  local -a stress_apps=(blinky displaydemo)

  # Build PAPKs for both apps in the same mode as the flashed firmware.
  local -a sa_args
  for sa in "${stress_apps[@]}"; do
    local apk_path="$REPO_ROOT/build/apks/${sa}.papk"
    hil_log "  Building PAPK for $sa ($mode)..."
    sa_args=(--app "$sa")
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
    local apk_path="$REPO_ROOT/build/apks/${sa}.papk"
    local remaining=$((deadline - SECONDS))

    echo "=== cycle $i/$total_cycles: installing $sa ===" >> "$log_file"
    hil_log "  cycle $i/$total_cycles: installing $sa"

    if timeout "$remaining" "$PDB_BIN" install "$apk_path" >> "$log_file" 2>&1; then
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

# Acquire exclusive lock (blocks if another run is in progress).
exec 9>"$HIL_LOCK"
hil_log "Waiting for hardware lock..."
flock 9
hil_log "Lock acquired."

# Pull latest code.
hil_log "Pulling latest code..."
git -C "$REPO_ROOT" pull --ff-only 2>&1 | while IFS= read -r line; do hil_log "  git: $line"; done || true

COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
RUN_ID="$(date '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT_SHA}"
RUN_LOG_DIR="$HIL_LOG_DIR/$RUN_ID"
RESULTS_FILE="$HIL_RESULTS_DIR/${RUN_ID}.txt"

mkdir -p "$RUN_LOG_DIR"

# Pre-build PDB host tool (needed for pdb category tests).
PDB_BIN=""
if [[ "$SKIP_PDB" != "true" ]]; then
  hil_log "Building PDB tool..."
  PDB_HOST_TARGET="$(host_target)"
  PDB_BIN="$REPO_ROOT/target/${PDB_HOST_TARGET}/release/pdb"
  if ! cargo build --release --quiet \
      --target "$PDB_HOST_TARGET" \
      --manifest-path "$REPO_ROOT/tools/pdb/Cargo.toml" \
      > "$RUN_LOG_DIR/pdb-build.log" 2>&1; then
    hil_log "WARNING: PDB tool build failed; PDB tests will be skipped"
    PDB_BIN=""
  fi
fi

hil_log "========================================="
hil_log "HIL Run: $RUN_ID"
hil_log "========================================="

PASS=0; FAIL=0; SKIP=0; ERROR=0; TOTAL=0

run_test() {
  local app="$1" category="$2" timeout="$3" patterns="$4" mode="$5"
  local tag="${app}[${mode}]"
  local log_file="$RUN_LOG_DIR/${app}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${app}.${mode}.build.log"

  TOTAL=$((TOTAL + 1))
  hil_log "--- [$TOTAL] $tag ($category, ${timeout}s) ---"

  # Power cycle devices to ensure clean state.
  power_cycle_all

  # Build APK (mode-tagged; --shrink iff this iteration is the shrunk one).
  hil_log "  Building APK..."
  local -a apk_args=(--app "$app")
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1; then
    hil_log "  BUILD FAILED (APK)"
    echo "ERROR $tag (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local apk_path="$REPO_ROOT/build/apks/${app}.papk"
  local jobs
  jobs=$(cpu_count)

  # Build firmware (release). PICODROID_SHRINK must match the APK's mode
  # or verify_compat will reject at load.
  hil_log "  Building firmware (release)..."
  local -a cargo_env=(PICODROID_APK_PATH="$apk_path")
  [[ "$mode" == "shrink" ]] && cargo_env+=(PICODROID_SHRINK=1)
  if ! env "${cargo_env[@]}" cargo build \
    --release \
    --jobs "$jobs" \
    --target "$TARGET" \
    --no-default-features \
    --features "$BOARD_FEATURE" >> "$build_log" 2>&1; then
    hil_log "  BUILD FAILED (firmware)"
    echo "ERROR $tag (firmware build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # Clean up any lingering probe-rs from previous test, then wait for probe.
  pkill -f "probe-rs" 2>/dev/null || true
  sleep 2

  # Flash the pre-built ELF and capture RTT output.
  local elf="$REPO_ROOT/target/${TARGET}/release/picodroid"
  hil_log "  Flashing and capturing RTT..."
  setsid timeout "$timeout" \
    probe-rs run --chip RP235x --protocol swd "$elf" \
    > "$log_file" 2>&1 &
  local run_pid=$!

  local result=1  # assume failure

  if [[ "$category" == "term" || "$category" == "hw" ]]; then
    # Poll for expected output, kill early on match.
    local elapsed=0
    while kill -0 "$run_pid" 2>/dev/null && [[ $elapsed -lt $timeout ]]; do
      sleep 1
      elapsed=$((elapsed + 1))
      if check_patterns "$log_file" "$patterns" > /dev/null 2>&1; then
        result=0
        break
      fi
    done
    kill_process_group "$run_pid"

  elif [[ "$category" == "loop" ]]; then
    # Let it run for the full timeout, then check patterns.
    wait "$run_pid" 2>/dev/null || true
    if check_patterns "$log_file" "$patterns" > /dev/null 2>&1; then
      result=0
    fi
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

  while IFS='|' read -r app category timeout patterns pdb_cmd; do
    # Skip comments and blank lines.
    [[ "$app" =~ ^[[:space:]]*# ]] && continue
    [[ -z "$app" ]] && continue

    # If specific app requested, skip others.
    if [[ -n "$SPECIFIC_APP" && "$app" != "$SPECIFIC_APP" ]]; then
      continue
    fi

    # Skip hw-dependent tests unless --include-hw.
    if [[ "$category" == "hw" && "$INCLUDE_HW" != "true" ]]; then
      hil_log "SKIP $app[$MODE] (hardware-dependent)"
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

    run_test "$app" "$category" "$timeout" "$patterns" "$MODE"
  done < "$HIL_CONF"
done

# Release lock.
flock -u 9

# Summary.
hil_log "========================================="
hil_log "HIL Run $RUN_ID Complete"
hil_log "  PASS: $PASS  FAIL: $FAIL  SKIP: $SKIP  ERROR: $ERROR"
hil_log "  Results: $RESULTS_FILE"
hil_log "  Logs:    $RUN_LOG_DIR/"
hil_log "========================================="

# Send email report.
if [[ "$SEND_EMAIL" == "true" ]]; then
  hil_log "Sending email report..."
  python3 "$SCRIPT_DIR/hil-email.py" \
    --results "$RESULTS_FILE" \
    --log-dir "$HIL_LOG_DIR" \
    --run-id "$RUN_ID" \
    --sha "$COMMIT_SHA" 2>&1 | while IFS= read -r line; do hil_log "  email: $line"; done || \
    hil_log "  Email sending failed (non-fatal)."
fi

# Exit with failure if any tests failed.
[[ $FAIL -eq 0 && $ERROR -eq 0 ]]
