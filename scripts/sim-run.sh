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
  local app="$1" category="$2" timeout="$3" patterns="$4" mode="$5" board="${6:-testbench_rp2350}"
  local board_feature="board-${board//_/-}"
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
    --features "sim,$board_feature" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # Run the pre-built binary directly (avoids a redundant cargo build check).
  # PICODROID_SIM_HEADLESS=1 skips minifb window creation so Activity-based
  # tests (callbacktest, displaydemo) run under CI without an X server.
  # Parity defaults (docs/parity-audit.md): the handle sanitizer aborts on
  # use-after-delete lookups the 64-bit sim otherwise hides (HAL-05).
  # PICODROID_PARITY_STRICT is inert here now that the simulator runs the real
  # kernel and `Thread.start` with it (THR-01 closed, M7) — it is still passed
  # because the flag survives in the `cargo test` backing, where a spawn is
  # still refused. Both overridable from the environment.
  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  #
  # `< /dev/null` is load-bearing. The caller feeds hil-tests.conf into the
  # `while read` loop's stdin, and a sim that reaches display init spawns a
  # control-channel thread that buffered-reads stdin. Sharing the fd, it
  # swallowed the rest of the conf and the loop exited early — every row
  # after executordemo (the first app that both draws and outlives the read)
  # silently never ran, in the nightly and in CI.
  sim_log "  Running (${timeout}s timeout)..."
  local exit_code=0
  if ! PICODROID_APK_PATH="$apk_path" PICODROID_SIM_HEADLESS=1 \
       PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
       PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
       timeout "$timeout" "$bin" > "$log_file" 2>&1 < /dev/null; then
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

# Board-matrix smoke (docs/parity-audit.md LVG-01/BRD-01): everything above
# runs on the testbench board, but the shipping enviro app has a different
# compile-time LVGL config (48 KB pool vs 64, 166 dpi vs 130, 240x240,
# buttons-only/no-touch). Build and boot it on its real board so
# board-conditional code is exercised in sim CI at all. The app loops
# forever; a timeout kill after a verified boot is the expected outcome.
run_enviro_smoke() {
  local mode="$1"
  local tag="picoenvmon-enviro[${mode}]"
  local log_file="$RUN_LOG_DIR/picoenvmon-enviro.${mode}.log"
  local build_log="$RUN_LOG_DIR/picoenvmon-enviro.${mode}.build.log"
  local patterns='PicoEnvMon[]:] Home.onCreate'

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag (board smoke, 25s) ---"

  local -a apk_args=(--app picoenvmon)
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (APK)"
    echo "ERROR $tag (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local -a cargo_env=(PICODROID_APK_PATH="sim-runtime")
  [[ "$mode" == "shrink" ]] && cargo_env+=(PICODROID_SHRINK=1)
  if ! env "${cargo_env[@]}" cargo build \
    --release \
    --target "$HOST_TARGET" \
    --no-default-features \
    --features "sim,board-pico-enviro-mon" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim, enviro board)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  PICODROID_APK_PATH="$REPO_ROOT/build/apks/picoenvmon.papk" \
    PICODROID_SIM_HEADLESS=1 \
    PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
    PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
    timeout 25 "$bin" > "$log_file" 2>&1 < /dev/null || true

  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1 \
     && check_no_crash "$log_file" > /dev/null 2>&1; then
    sim_log "  PASS"
    echo "PASS $tag" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    echo "FAIL $tag" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}

# WiFi-variant board smoke: same app on pico_enviro_mon_w, which is the only
# board combining sensors + network. Boots headless, then probes the
# dashboard HTTP server over the sim's host-network passthrough. NTP and
# weather need real internet, so the assertion accepts EITHER outcome token
# (synced or the fail-soft path) — nightly must not depend on the internet.
run_enviro_w_smoke() {
  local mode="$1"
  local tag="picoenvmon-enviro-w[${mode}]"
  local log_file="$RUN_LOG_DIR/picoenvmon-enviro-w.${mode}.log"
  local build_log="$RUN_LOG_DIR/picoenvmon-enviro-w.${mode}.build.log"
  local patterns='PicoEnvMon[]:] Home.onCreate;net: up;http: serving on port 8080'

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag (WiFi board smoke, 25s) ---"

  local -a apk_args=(--app picoenvmon)
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (APK)"
    echo "ERROR $tag (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local -a cargo_env=(PICODROID_APK_PATH="sim-runtime")
  [[ "$mode" == "shrink" ]] && cargo_env+=(PICODROID_SHRINK=1)
  if ! env "${cargo_env[@]}" cargo build \
    --release \
    --target "$HOST_TARGET" \
    --no-default-features \
    --features "sim,board-pico-enviro-mon-w" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim, enviro-w board)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  PICODROID_APK_PATH="$REPO_ROOT/build/apks/picoenvmon.papk" \
    PICODROID_SIM_HEADLESS=1 \
    PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
    PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
    timeout 25 "$bin" > "$log_file" 2>&1 < /dev/null &
  local sim_pid=$!

  # Probe the dashboard once the server line appears (bounded wait). The
  # serve loop shares its thread with NTP + weather housekeeping (bounded but
  # up to ~11 s on a slow external endpoint), so retry rather than fail on one
  # unanswered request — the 2026-08-18 nightly failed on exactly that.
  local page_ok=0
  local i attempt
  for i in $(seq 1 20); do
    if grep -q "http: serving" "$log_file" 2>/dev/null; then
      for attempt in 1 2 3; do
        if curl -sf -m 5 "http://127.0.0.1:8080/" 2>/dev/null | grep -q "PicoEnvMon"; then
          page_ok=1
          break
        fi
        sleep 2
      done
      break
    fi
    sleep 1
  done
  wait "$sim_pid" 2>/dev/null || true

  if [[ "$page_ok" == "1" ]] \
     && check_patterns "$log_file" "$patterns" > /dev/null 2>&1 \
     && check_no_crash "$log_file" > /dev/null 2>&1; then
    sim_log "  PASS"
    echo "PASS $tag" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL (page_ok=$page_ok)"
    tail -5 "$log_file" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    echo "FAIL $tag" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}

# Run every selected test once per shrink mode.
for MODE in "${MODES[@]}"; do
  sim_log "========================================="
  sim_log "Mode: $MODE"
  sim_log "========================================="

  # Parse config and run tests. The 5th column is the pdb command for pdb
  # rows and an optional board override for sim rows (e.g. netexception
  # needs the network-enabled W board's sim build).
  while IFS='|' read -r app category timeout patterns extra; do
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

    if [[ "$category" == "sim" && -n "${extra:-}" ]]; then
      run_test "$app" "$category" "$timeout" "$patterns" "$MODE" "$extra"
    else
      run_test "$app" "$category" "$timeout" "$patterns" "$MODE"
    fi
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

  # Memory-diagnostics soak (docs/memory-diagnostics.md): strict growth
  # sentinel + offensive checks + detector self-test. Shrink-invariant, so
  # one pass per cycle is enough.
  if [[ -z "$SPECIFIC_APP" && "$MODE" != "shrink" ]]; then
    TOTAL=$((TOTAL + 1))
    sim_log "--- [$TOTAL] mem-diag soak ---"
    memdiag_log="$RUN_LOG_DIR/mem-diag.log"
    if bash "$SCRIPT_DIR/test-memdiag.sh" > "$memdiag_log" 2>&1; then
      sim_log "  PASS"
      echo "PASS mem-diag" >> "$RESULTS_FILE"
      PASS=$((PASS + 1))
    else
      sim_log "  FAIL"
      tail -10 "$memdiag_log" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
      echo "FAIL mem-diag" >> "$RESULTS_FILE"
      FAIL=$((FAIL + 1))
    fi
  fi

  # Enviro-board smoke: full runs and `--app picoenvmon` (the CI hook; the
  # conf matrix has no picoenvmon row, so that invocation reaches only this).
  if [[ -z "$SPECIFIC_APP" || "$SPECIFIC_APP" == "picoenvmon" ]]; then
    run_enviro_smoke "$MODE"
    run_enviro_w_smoke "$MODE"
  fi
done

# Summary.
sim_log "========================================="
sim_log "Sim Run $RUN_ID Complete"
sim_log "  PASS: $PASS  FAIL: $FAIL  SKIP: $SKIP  ERROR: $ERROR"
sim_log "  Results: $RESULTS_FILE"
sim_log "  Logs:    $RUN_LOG_DIR/"
sim_log "========================================="

# Append this run's benchmark metrics to bench/parity/history.csv and print a
# drift summary. Informational only -- a slow perf regression should surface in
# the morning report, but it must never fail the correctness run.
# (docs/perf-campaign-2026-08.md S0)
if [[ -x "$SCRIPT_DIR/bench-backfill.py" ]]; then
  sim_log "Appending benchmark metrics to bench/parity/history.csv..."
  python3 "$SCRIPT_DIR/bench-backfill.py" --run-dir "$RUN_LOG_DIR" --quiet 2>&1 |
    while IFS= read -r line; do sim_log "  bench: $line"; done || \
    sim_log "  Metric backfill failed (non-fatal)."
  python3 "$SCRIPT_DIR/bench-report.py" --trend wall_ms --env sim \
    --app benchmark --mode no-shrink 2>/dev/null | tail -6 |
    while IFS= read -r line; do sim_log "  trend: $line"; done || true
  # Keep the working tree clean. These runs append to a tracked file, so
  # without this every morning starts dirty -- and the reflex response to a
  # dirty generated file, `git checkout --`, would silently destroy any row
  # no log can rebuild (an ad-hoc parity-bench run whose temp log directory
  # is gone, or the pre-2026-08-28 size rows).
  #
  # Commits that one path only, via a pathspec commit rather than `git add`,
  # so nothing else a human left staged gets swept in. --no-verify because the
  # hook is the 15-minute suite and this is a data append, not a code change.
  # Never pushes: the cron's own `git pull --ff-only` stays happy with
  # unpushed local commits unless the remote diverged, and pushing is a
  # decision for a human.
  # Scheduled runs commit; verification runs do not. scripts/pre-commit drives
  # sim-run.sh for its langsuite stage, and a test suite that silently creates
  # a commit is a nasty surprise -- it sets this to 0.
  if [[ "${PICODROID_BENCH_AUTOCOMMIT:-1}" == "1" ]] &&
     ! git -C "$REPO_ROOT" diff --quiet -- bench/parity/history.csv 2>/dev/null; then
    if git -C "$REPO_ROOT" commit -q --no-verify \
         -m "chore(bench): sim metrics for $RUN_ID" \
         -- bench/parity/history.csv 2>/dev/null; then
      sim_log "  bench: committed metrics for $RUN_ID"
    else
      sim_log "  bench: metric commit failed (non-fatal, tree left dirty)"
    fi
  fi
fi

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
