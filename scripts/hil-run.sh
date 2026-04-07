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

USB_HUB="1-9.3"
USB_PORT_PROBE=3
USB_PORT_PICO=2

TARGET="thumbv8m.main-none-eabihf"
CHIP_FEATURE="chip-rp2350"

INCLUDE_HW=false
SPECIFIC_APP=""
SEND_EMAIL=true

# ── Argument parsing ────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --include-hw) INCLUDE_HW=true; shift ;;
    --no-email)   SEND_EMAIL=false; shift ;;
    --app)        SPECIFIC_APP="$2"; shift 2 ;;
    -h|--help)
      cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --app <name>    Run only the specified test
  --include-hw    Also run hardware-peripheral tests (adcdemo, i2cdemo, etc.)
  --no-email      Skip sending the email report
  -h, --help      Show this help message
EOF
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

# ── Helpers ──────────────────────────────────────────────────────────────────

hil_log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

PROBE_SYSFS="/sys/bus/usb/devices/${USB_HUB}.${USB_PORT_PROBE}"
PROBE_POLL_INTERVAL=1
PROBE_POLL_TIMEOUT=15

power_cycle_all() {
  hil_log "Power-cycling debug probe + Pico 2..."
  if ! sudo uhubctl -l "$USB_HUB" -p "$USB_PORT_PICO,$USB_PORT_PROBE" -a cycle 2>&1 | \
       while IFS= read -r line; do hil_log "  uhubctl: $line"; done; then
    hil_log "  WARNING: uhubctl returned non-zero exit status"
  fi
  sleep 5
  wait_for_probe
}

# Poll until the debug probe re-enumerates on USB.
wait_for_probe() {
  local elapsed=0
  while [[ ! -e "$PROBE_SYSFS" ]] && [[ $elapsed -lt $PROBE_POLL_TIMEOUT ]]; do
    sleep "$PROBE_POLL_INTERVAL"
    elapsed=$((elapsed + PROBE_POLL_INTERVAL))
  done
  if [[ -e "$PROBE_SYSFS" ]]; then
    hil_log "  Probe re-enumerated after ${elapsed}s"
  else
    hil_log "  WARNING: Probe did not re-enumerate within ${PROBE_POLL_TIMEOUT}s ($PROBE_SYSFS missing)"
  fi
}

recover_probe() {
  hil_log "Recovering probe..."
  pkill -f "probe-rs" 2>/dev/null || true
  sleep 1
  power_cycle_all
}

# Check if all expected patterns are found in a log file.
# Args: log_file "pattern1;pattern2;..."
# Prints missing patterns to stdout; returns 0 if all found, 1 if any missing.
check_patterns() {
  local log_file="$1"
  local patterns="$2"
  local missing=0

  IFS=';' read -ra PATS <<< "$patterns"
  for pat in "${PATS[@]}"; do
    [[ -z "$pat" ]] && continue
    if ! grep -qE "$pat" "$log_file" 2>/dev/null; then
      echo "  MISSING: $pat"
      missing=1
    fi
  done
  return $missing
}

# Kill an entire process group started with setsid.
# Args: pid
kill_process_group() {
  local pid="$1"
  kill -- -"$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
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

hil_log "========================================="
hil_log "HIL Run: $RUN_ID"
hil_log "========================================="

PASS=0; FAIL=0; SKIP=0; ERROR=0; TOTAL=0

run_test() {
  local app="$1" category="$2" timeout="$3" patterns="$4"
  local log_file="$RUN_LOG_DIR/${app}.log"
  local build_log="$RUN_LOG_DIR/${app}.build.log"

  TOTAL=$((TOTAL + 1))
  hil_log "--- [$TOTAL] $app ($category, ${timeout}s) ---"

  # Power cycle devices to ensure clean state.
  power_cycle_all

  # Build APK.
  hil_log "  Building APK..."
  if ! bash "$SCRIPT_DIR/build-apk.sh" --app "$app" > "$build_log" 2>&1; then
    hil_log "  BUILD FAILED (APK)"
    echo "ERROR $app (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local apk_path="$REPO_ROOT/build/apks/${app}.papk"
  local jobs
  jobs=$(cpu_count)

  # Build firmware (release).
  hil_log "  Building firmware (release)..."
  if ! PICODROID_APK_PATH="$apk_path" cargo build \
    --release \
    --jobs "$jobs" \
    --target "$TARGET" \
    --no-default-features \
    --features "$CHIP_FEATURE" >> "$build_log" 2>&1; then
    hil_log "  BUILD FAILED (firmware)"
    echo "ERROR $app (firmware build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # Clean up any lingering probe-rs from previous test, then wait for probe.
  pkill -f "probe-rs" 2>/dev/null || true
  sleep 2

  # Flash and capture RTT output.
  hil_log "  Flashing and capturing RTT..."
  setsid bash -c "
    PICODROID_APK_PATH='$apk_path' timeout $timeout cargo run \
      --release \
      --jobs \$(nproc 2>/dev/null || sysctl -n hw.logicalcpu) \
      --target '$TARGET' \
      --no-default-features \
      --features '$CHIP_FEATURE' 2>&1
  " > "$log_file" 2>&1 &
  local run_pid=$!

  local result=1  # assume failure

  if [[ "$category" == "term" ]]; then
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
    echo "PASS $app" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    hil_log "  FAIL"
    hil_log "  Log tail:"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do hil_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do hil_log "  $line"; done || true
    echo "FAIL $app" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))

    # Try to recover for next test.
    recover_probe
  fi
}

# Parse config and run tests.
while IFS='|' read -r app category timeout patterns; do
  # Skip comments and blank lines.
  [[ "$app" =~ ^[[:space:]]*# ]] && continue
  [[ -z "$app" ]] && continue

  # If specific app requested, skip others.
  if [[ -n "$SPECIFIC_APP" && "$app" != "$SPECIFIC_APP" ]]; then
    continue
  fi

  # Skip hw-dependent tests unless --include-hw.
  if [[ "$category" == "hw" && "$INCLUDE_HW" != "true" ]]; then
    hil_log "SKIP $app (hardware-dependent)"
    echo "SKIP $app" >> "$RESULTS_FILE"
    SKIP=$((SKIP + 1))
    continue
  fi

  # Skip explicitly skipped tests.
  if [[ "$category" == "skip" ]]; then
    hil_log "SKIP $app"
    echo "SKIP $app" >> "$RESULTS_FILE"
    SKIP=$((SKIP + 1))
    continue
  fi

  run_test "$app" "$category" "$timeout" "$patterns"
done < "$HIL_CONF"

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
