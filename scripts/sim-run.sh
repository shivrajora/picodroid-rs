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
# Covers the --shrink matrix: every test runs once without shrinking (the
# default runtime behavior) and once with it. Override with --mode if you
# want to inspect a single side.
MODES=("no-shrink" "shrink")

# ── Argument parsing ────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)        SPECIFIC_APP="$2"; shift 2 ;;
    --no-email)   SEND_EMAIL=false; shift ;;
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
  --app <name>            Run only the specified test
  --mode <no-shrink|shrink|both>
                          Shrink modes to exercise (default: both). Every
                          selected test is run once per mode so regressions
                          on either side are caught.
  --no-email              Skip sending the email report
  -h, --help              Show this help message
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
  local app="$1" category="$2" timeout="$3" patterns="$4" mode="$5"
  local tag="${app}[${mode}]"
  local log_file="$RUN_LOG_DIR/${app}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${app}.${mode}.build.log"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag ($category, ${timeout}s) ---"

  # Build APK.
  sim_log "  Building APK..."
  local -a apk_args=(--app "$app")
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (APK)"
    echo "ERROR $tag (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local apk_path="$REPO_ROOT/build/apks/${app}.papk"

  # Build sim binary (release). PICODROID_SHRINK must match the APK's mode
  # or verify_compat will reject at load time.
  #
  # The build-time PICODROID_APK_PATH is a constant marker, not the real
  # path: sim binaries load the .papk at startup from the *runtime* env var
  # (see build_support/papk.rs::embed_apk), and the framework-class embed
  # only keys on the var being set. A stable value means the first build per
  # mode is the only real build — switching apps is a cargo no-op.
  sim_log "  Building sim binary..."
  local -a cargo_env=(PICODROID_APK_PATH="sim-runtime")
  [[ "$mode" == "shrink" ]] && cargo_env+=(PICODROID_SHRINK=1)
  if ! env "${cargo_env[@]}" cargo build \
    --release \
    --target "$HOST_TARGET" \
    --no-default-features \
    --features "sim,board-testbench-rp2350" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # Run the pre-built binary directly (avoids a redundant cargo build check).
  # PICODROID_SIM_HEADLESS=1 skips minifb window creation so Activity-based
  # tests (callbacktest, displaydemo) run under CI without an X server.
  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  sim_log "  Running (${timeout}s timeout)..."
  local exit_code=0
  if ! PICODROID_APK_PATH="$apk_path" PICODROID_SIM_HEADLESS=1 \
       timeout "$timeout" "$bin" > "$log_file" 2>&1; then
    exit_code=$?
  fi

  # Non-loop tests must complete within their timeout; exit 124 there means
  # the app hung or deadlocked rather than produced wrong output. Classify as
  # ERROR so triage distinguishes "didn't finish" from "finished, wrong log".
  if [[ $exit_code -eq 124 && "$category" != "loop" ]]; then
    sim_log "  TIMED OUT (no completion within ${timeout}s)"
    echo "ERROR $tag (timed out)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # Check positive patterns AND absence of crash markers. Without the crash
  # scan, an app that prints the expected token then panics would still PASS.
  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1 \
     && check_no_crash "$log_file" > /dev/null 2>&1; then
    sim_log "  PASS"
    echo "PASS $tag" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL"
    sim_log "  Log tail:"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
    check_no_crash "$log_file" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
    echo "FAIL $tag" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}

# Run every selected test once per shrink mode.
for MODE in "${MODES[@]}"; do
  sim_log "========================================="
  sim_log "Mode: $MODE"
  sim_log "========================================="

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
      sim_log "SKIP $app[$MODE] (hardware-dependent)"
      echo "SKIP $app[$MODE]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # Skip pdb tests (require a real device on USB CDC).
    if [[ "$category" == "pdb" ]]; then
      sim_log "SKIP $app[$MODE] (pdb — requires device)"
      echo "SKIP $app[$MODE]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # Skip explicitly skipped tests.
    if [[ "$category" == "skip" ]]; then
      sim_log "SKIP $app[$MODE]"
      echo "SKIP $app[$MODE]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    run_test "$app" "$category" "$timeout" "$patterns" "$MODE"
  done < "$SIM_CONF"

  # Heap pressure tests (sim-based; bundled here so they run on every sim cycle
  # instead of slowing down pre-commit). Also mode-varied to catch any shrink
  # regressions in the allocator path.
  if [[ -z "$SPECIFIC_APP" ]]; then
    TOTAL=$((TOTAL + 1))
    sim_log "--- [$TOTAL] heap-pressure[$MODE] ---"
    heap_log="$RUN_LOG_DIR/heap-pressure.${MODE}.log"
    heap_env=()
    [[ "$MODE" == "shrink" ]] && heap_env+=(PICODROID_SHRINK=1)
    if env "${heap_env[@]}" bash "$SCRIPT_DIR/test-heap.sh" > "$heap_log" 2>&1; then
      sim_log "  PASS"
      echo "PASS heap-pressure[$MODE]" >> "$RESULTS_FILE"
      PASS=$((PASS + 1))
    else
      sim_log "  FAIL"
      tail -10 "$heap_log" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
      echo "FAIL heap-pressure[$MODE]" >> "$RESULTS_FILE"
      FAIL=$((FAIL + 1))
    fi
  fi
done

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
