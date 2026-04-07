#!/usr/bin/env bash
# Simulator test runner for picodroid.
#
# Builds and runs each example app in sim mode (release), verifies expected
# log patterns from hil-tests.conf.
#
# Usage:
#   ./scripts/sim-run.sh                  # run all sim-compatible tests
#   ./scripts/sim-run.sh --app helloworld # run one test only
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

# ── Argument parsing ────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)        SPECIFIC_APP="$2"; shift 2 ;;
    -h|--help)
      cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --app <name>    Run only the specified test
  -h, --help      Show this help message
EOF
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

# ── Helpers ─────────────────────────────────────────────────────────────────

sim_log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

# Check if all expected patterns are found in a log file.
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

# ── Main ────────────────────────────────────────────────────────────────────

mkdir -p "$SIM_LOG_DIR" "$SIM_RESULTS_DIR"

# Pull latest code.
sim_log "Pulling latest code..."
git -C "$REPO_ROOT" pull --ff-only 2>&1 | while IFS= read -r line; do sim_log "  git: $line"; done || true

COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
RUN_ID="$(date '+%Y%m%d-%H%M%S')-${COMMIT_SHA}"
RESULTS_FILE="$SIM_RESULTS_DIR/${RUN_ID}.txt"

sim_log "========================================="
sim_log "Sim Run: $RUN_ID"
sim_log "========================================="

PASS=0; FAIL=0; SKIP=0; TOTAL=0

run_test() {
  local app="$1" category="$2" timeout="$3" patterns="$4"
  local log_file="$SIM_LOG_DIR/${RUN_ID}-${app}.log"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $app ($category, ${timeout}s) ---"

  # Run sim with timeout.
  sim_log "  Running sim (release)..."
  if timeout "$timeout" bash "$SCRIPT_DIR/sim.sh" --app "$app" --release > "$log_file" 2>&1; then
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

# Summary.
sim_log "========================================="
sim_log "Sim Run $RUN_ID Complete"
sim_log "  PASS: $PASS  FAIL: $FAIL  SKIP: $SKIP"
sim_log "  Results: $RESULTS_FILE"
sim_log "  Logs:    $SIM_LOG_DIR/${RUN_ID}-*.log"
sim_log "========================================="

# Exit with failure if any tests failed.
[[ $FAIL -eq 0 ]]
