#!/usr/bin/env bash
# Simulator test runner for picodroid.
#
# Builds and runs each example app in sim mode (release), verifies expected
# log patterns from hil-tests.conf.
#
# Usage:
#   ./scripts/sim-run.sh                  # run all sim-compatible tests, send email report
#   ./scripts/sim-run.sh --app helloworld # run one test only
#   ./scripts/sim-run.sh --no-email       # skip email report
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

SIM_CONF="$SCRIPT_DIR/hil-tests.conf"
SIM_DIR="$REPO_ROOT/build/sim"
SIM_LOG_DIR="$SIM_DIR/logs"
SIM_RESULTS_DIR="$SIM_DIR/results"

SPECIFIC_APP=""
SEND_EMAIL=true

# ── Argument parsing ────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)        SPECIFIC_APP="$2"; shift 2 ;;
    --no-email)   SEND_EMAIL=false; shift ;;
    -h|--help)
      cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --app <name>    Run only the specified test
  --no-email      Skip sending the email report
  -h, --help      Show this help message
EOF
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

# ── Helpers ─────────────────────────────────────────────────────────────────

sim_log() { timestamp_log "$@"; }

# ── Main ────────────────────────────────────────────────────────────────────

mkdir -p "$SIM_LOG_DIR" "$SIM_RESULTS_DIR"

# Pull latest code.
sim_log "Pulling latest code..."
git -C "$REPO_ROOT" pull --ff-only 2>&1 | while IFS= read -r line; do sim_log "  git: $line"; done || true

COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
RUN_ID="$(date '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT_SHA}"
RUN_LOG_DIR="$SIM_LOG_DIR/$RUN_ID"
RESULTS_FILE="$SIM_RESULTS_DIR/${RUN_ID}.txt"

mkdir -p "$RUN_LOG_DIR"

sim_log "========================================="
sim_log "Sim Run: $RUN_ID"
sim_log "========================================="

PASS=0; FAIL=0; SKIP=0; ERROR=0; TOTAL=0

HOST_TARGET="$(host_target)"

run_test() {
  local app="$1" category="$2" timeout="$3" patterns="$4"
  local log_file="$RUN_LOG_DIR/${app}.log"
  local build_log="$RUN_LOG_DIR/${app}.build.log"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $app ($category, ${timeout}s) ---"

  # Build APK.
  sim_log "  Building APK..."
  if ! bash "$SCRIPT_DIR/build-apk.sh" --app "$app" > "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (APK)"
    echo "ERROR $app (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local apk_path="$REPO_ROOT/build/apks/${app}.papk"

  # Build sim binary (release).
  sim_log "  Building sim binary..."
  if ! PICODROID_APK_PATH="$apk_path" cargo build \
    --release \
    --target "$HOST_TARGET" \
    --no-default-features \
    --features "sim,board-testbench-rp2350" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim)"
    echo "ERROR $app (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # Run the pre-built binary directly (avoids a redundant cargo build check).
  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  sim_log "  Running (${timeout}s timeout)..."
  if PICODROID_APK_PATH="$apk_path" timeout "$timeout" "$bin" > "$log_file" 2>&1; then
    : # exited cleanly
  else
    local exit_code=$?
    # 124 = timeout killed it, which is expected for "loop" category apps.
    if [[ "$category" == "loop" && $exit_code -eq 124 ]]; then
      : # expected
    elif [[ $exit_code -eq 124 ]]; then
      sim_log "  TIMED OUT"
    fi
  fi

  # Check patterns.
  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1; then
    sim_log "  PASS"
    echo "PASS $app" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL"
    sim_log "  Log tail:"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
    echo "FAIL $app" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
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

  # Skip hw-dependent tests (no hardware in sim).
  if [[ "$category" == "hw" ]]; then
    sim_log "SKIP $app (hardware-dependent)"
    echo "SKIP $app" >> "$RESULTS_FILE"
    SKIP=$((SKIP + 1))
    continue
  fi

  # Skip explicitly skipped tests.
  if [[ "$category" == "skip" ]]; then
    sim_log "SKIP $app"
    echo "SKIP $app" >> "$RESULTS_FILE"
    SKIP=$((SKIP + 1))
    continue
  fi

  run_test "$app" "$category" "$timeout" "$patterns"
done < "$SIM_CONF"

# Heap pressure tests (sim-based; bundled here so they run on every sim cycle
# instead of slowing down pre-commit).
if [[ -z "$SPECIFIC_APP" ]]; then
  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] heap-pressure ---"
  heap_log="$RUN_LOG_DIR/heap-pressure.log"
  if bash "$SCRIPT_DIR/test-heap.sh" > "$heap_log" 2>&1; then
    sim_log "  PASS"
    echo "PASS heap-pressure" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL"
    tail -10 "$heap_log" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    echo "FAIL heap-pressure" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
fi

# Summary.
sim_log "========================================="
sim_log "Sim Run $RUN_ID Complete"
sim_log "  PASS: $PASS  FAIL: $FAIL  SKIP: $SKIP  ERROR: $ERROR"
sim_log "  Results: $RESULTS_FILE"
sim_log "  Logs:    $RUN_LOG_DIR/"
sim_log "========================================="

# Send email report.
if [[ "$SEND_EMAIL" == "true" ]]; then
  sim_log "Sending email report..."
  python3 "$SCRIPT_DIR/hil-email.py" \
    --results "$RESULTS_FILE" \
    --log-dir "$SIM_LOG_DIR" \
    --run-id "$RUN_ID" \
    --sha "$COMMIT_SHA" \
    --suite sim 2>&1 | while IFS= read -r line; do sim_log "  email: $line"; done || \
    sim_log "  Email sending failed (non-fatal)."
fi

# Exit with failure if any tests failed or errored.
[[ $FAIL -eq 0 && $ERROR -eq 0 ]]
