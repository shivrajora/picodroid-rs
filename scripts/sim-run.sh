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
# Architectures to run the matrix on. "host" is the native build this runner
# has always done. "arm32" rebuilds the sim for 32-bit ARM Linux and runs it
# under qemu-user, which is the only way this suite sees the device's pointer
# width — the handle table, the socket tables and every `usize` offset take
# their 32-bit arm there rather than the 64-bit one.
ARCHES=("host")
# qemu-user's TCG interpreter runs several times slower than native. Scale the
# per-test timeouts from hil-tests.conf rather than editing the conf, which is
# shared with the hardware runner and describes real device budgets.
ARM32_TIMEOUT_MULT="${PICODROID_ARM32_TIMEOUT_MULT:-5}"
# Wall-clock hog whose numbers are meaningless under emulation anyway.
ARM32_SKIP_APPS=("benchmark")

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
    --arch)
      case "$2" in
        host)  ARCHES=("host") ;;
        arm32) ARCHES=("arm32") ;;
        both)  ARCHES=("host" "arm32") ;;
        *) echo "Unknown --arch value: $2 (want host|arm32|both)" >&2; exit 1 ;;
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
  --arch <host|arm32|both>
                          Architectures to exercise (default: host). 'arm32'
                          builds the sim for 32-bit ARM Linux and runs it
                          under qemu-user, giving the suite the device's
                          pointer width and instruction set. Results are
                          tagged app[mode/arm32]. Needs qemu-user-static +
                          gcc-arm-linux-gnueabihf.
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
  local app="$1" category="$2" timeout="$3" patterns="$4" mode="$5" arch="${6:-host}"
  # Host tags keep their historical shape so the results history in
  # build/sim/results stays comparable run to run; other arches get a suffix.
  # hil-email.py treats the whole tag as an opaque key, so this needs no
  # change there.
  local suffix="" target="$HOST_TARGET"
  if [[ "$arch" != "host" ]]; then
    suffix="/${arch}"
    target="$(sim_target "$arch")"
    timeout=$((timeout * ARM32_TIMEOUT_MULT))
  fi
  local tag="${app}[${mode}${suffix}]"
  local slug="${app}.${mode}${suffix//\//-}"
  local log_file="$RUN_LOG_DIR/${slug}.log"
  local build_log="$RUN_LOG_DIR/${slug}.build.log"

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
  # `-p picodroid` matters off the host: a bare workspace build would also
  # try tools/pdb, whose serialport -> libudev-sys needs an armhf pkg-config
  # sysroot. Naming the binary keeps the cross build to portable crates.
  if ! env "${cargo_env[@]}" cargo build \
    -p picodroid \
    --release \
    --target "$target" \
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
  # Parity defaults (docs/parity-audit.md): the handle sanitizer aborts on
  # use-after-delete lookups the 64-bit sim otherwise hides (HAL-05), and
  # parity-strict turns silent sim no-ops (Thread.start, THR-01) into hard
  # failures so an app whose code never ran cannot PASS. Both overridable
  # from the environment.
  #
  # On arm32 the binary is ARM, so it goes through qemu explicitly rather
  # than relying on a binfmt_misc registration being present. It also gets
  # its own filesystem image: the default path is baked in at compile time,
  # and sharing one image across arches would let bootcount/prefs state from
  # one lane leak into the other's assertions.
  local bin="$REPO_ROOT/target/$target/release/picodroid"
  local -a runner=()
  local -a extra_env=()
  if [[ "$arch" == "arm32" ]]; then
    runner=(qemu-arm-static -L /usr/arm-linux-gnueabihf)
    extra_env=(PICODROID_SIM_FS="$REPO_ROOT/platforms/rp/target/sim-fs.arm32.img")
  fi
  #
  # `< /dev/null` is load-bearing. The caller feeds hil-tests.conf into the
  # `while read` loop's stdin, and a sim that reaches display init spawns a
  # control-channel thread that buffered-reads stdin. Sharing the fd, it
  # swallowed the rest of the conf and the loop exited early — every row
  # after executordemo (the first app that both draws and outlives the read)
  # silently never ran, in the nightly and in CI.
  sim_log "  Running (${timeout}s timeout)..."
  local exit_code=0
  if ! env PICODROID_APK_PATH="$apk_path" PICODROID_SIM_HEADLESS=1 \
       PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
       PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
       "${extra_env[@]}" \
       timeout "$timeout" "${runner[@]}" "$bin" > "$log_file" 2>&1 < /dev/null; then
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
  local mode="$1" arch="${2:-host}"
  local suffix="" target="$HOST_TARGET" run_timeout=25
  if [[ "$arch" != "host" ]]; then
    suffix="/${arch}"
    target="$(sim_target "$arch")"
    run_timeout=$((run_timeout * ARM32_TIMEOUT_MULT))
  fi
  local tag="picoenvmon-enviro[${mode}${suffix}]"
  local slug="picoenvmon-enviro.${mode}${suffix//\//-}"
  local log_file="$RUN_LOG_DIR/${slug}.log"
  local build_log="$RUN_LOG_DIR/${slug}.build.log"
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
    -p picodroid \
    --release \
    --target "$target" \
    --no-default-features \
    --features "sim,board-pico-enviro-mon" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim, enviro board)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$target/release/picodroid"
  local -a runner=() extra_env=()
  if [[ "$arch" == "arm32" ]]; then
    runner=(qemu-arm-static -L /usr/arm-linux-gnueabihf)
    extra_env=(PICODROID_SIM_FS="$REPO_ROOT/platforms/rp/target/sim-fs.arm32-enviro.img")
  fi
  env PICODROID_APK_PATH="$REPO_ROOT/build/apks/picoenvmon.papk" \
    PICODROID_SIM_HEADLESS=1 \
    PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
    PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
    "${extra_env[@]}" \
    timeout "$run_timeout" "${runner[@]}" "$bin" > "$log_file" 2>&1 < /dev/null || true

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

if [[ " ${ARCHES[*]} " == *" arm32 "* ]] && ! have_arm32_sim; then
  sim_log "arm32 lane requested but the toolchain is incomplete."
  arm32_sim_hint
  exit 1
fi

# Run every selected test once per architecture, per shrink mode. Host runs
# first so an ordinary regression is reported before the slower emulated lane.
for ARCH in "${ARCHES[@]}"; do
# Host tags stay exactly as they were before this loop existed, so the
# results history stays comparable.
TAGSUF=""; [[ "$ARCH" != "host" ]] && TAGSUF="/$ARCH"
for MODE in "${MODES[@]}"; do
  sim_log "========================================="
  sim_log "Mode: $MODE${TAGSUF:+  Arch: $ARCH}"
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
      sim_log "SKIP $app[$MODE$TAGSUF] (hardware-dependent)"
      echo "SKIP $app[$MODE$TAGSUF]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # Skip pdb tests (require a real device on USB CDC).
    if [[ "$category" == "pdb" ]]; then
      sim_log "SKIP $app[$MODE$TAGSUF] (pdb — requires device)"
      echo "SKIP $app[$MODE$TAGSUF]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # Skip explicitly skipped tests.
    if [[ "$category" == "skip" ]]; then
      sim_log "SKIP $app[$MODE$TAGSUF]"
      echo "SKIP $app[$MODE$TAGSUF]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # Thread.start is a no-op in the sim (docs/parity-audit.md THR-01), so a
    # threaddemo "PASS" would certify a run in which no thread ever executed.
    # Under the parity-strict default the run aborts anyway; skip with an
    # honest reason instead. Re-enable when M7 (real sim threads) lands.
    if [[ "$app" == "threaddemo" && "${PICODROID_PARITY_STRICT:-1}" == "1" ]]; then
      sim_log "SKIP $app[$MODE$TAGSUF] (Thread.start no-op in sim — parity-strict, THR-01)"
      echo "SKIP $app[$MODE$TAGSUF]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    # Apps whose result is a wall-clock number, which emulation makes
    # meaningless (and which would run for the timeout x multiplier).
    if [[ "$ARCH" == "arm32" && " ${ARM32_SKIP_APPS[*]} " == *" $app "* ]]; then
      sim_log "SKIP $app[$MODE$TAGSUF] (timing-based — not meaningful under emulation)"
      echo "SKIP $app[$MODE$TAGSUF]" >> "$RESULTS_FILE"
      SKIP=$((SKIP + 1))
      continue
    fi

    run_test "$app" "$category" "$timeout" "$patterns" "$MODE" "$ARCH"
  done < "$SIM_CONF"

  # Heap pressure tests (sim-based; bundled here so they run on every sim cycle
  # instead of slowing down pre-commit). Also mode-varied to catch any shrink
  # regressions in the allocator path.
  #
  # Host only: this shells out to test-heap.sh, which drives sim.sh on the
  # native target. The OOM thresholds it asserts were measured at 64-bit
  # object sizes and would need re-measuring before they mean anything on a
  # 32-bit build, where the same app needs less heap.
  if [[ -z "$SPECIFIC_APP" && "$ARCH" == "host" ]]; then
    TOTAL=$((TOTAL + 1))
    sim_log "--- [$TOTAL] heap-pressure[$MODE$TAGSUF] ---"
    heap_log="$RUN_LOG_DIR/heap-pressure.${MODE}.log"
    heap_env=()
    [[ "$MODE" == "shrink" ]] && heap_env+=(PICODROID_SHRINK=1)
    if env "${heap_env[@]}" bash "$SCRIPT_DIR/test-heap.sh" > "$heap_log" 2>&1; then
      sim_log "  PASS"
      echo "PASS heap-pressure[$MODE$TAGSUF]" >> "$RESULTS_FILE"
      PASS=$((PASS + 1))
    else
      sim_log "  FAIL"
      tail -10 "$heap_log" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
      echo "FAIL heap-pressure[$MODE$TAGSUF]" >> "$RESULTS_FILE"
      FAIL=$((FAIL + 1))
    fi
  fi

  # Memory-diagnostics soak (docs/memory-diagnostics.md): strict growth
  # sentinel + offensive checks + detector self-test. Shrink-invariant, so
  # one pass per cycle is enough. Host only: the sentinel and the slow-handler
  # watchdog are millisecond thresholds, and emulation would trip them for
  # reasons that say nothing about the code under test.
  if [[ -z "$SPECIFIC_APP" && "$MODE" != "shrink" && "$ARCH" == "host" ]]; then
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
    run_enviro_smoke "$MODE" "$ARCH"
  fi
done

# One extra arm32 leg: the generational handle table compiled for 32 bits.
#
# The devices ship the other path — a widget handle is the raw pointer, so it
# survives the widget it names and a stale lookup reads freed memory. The
# table behind handle-table-32 fixes that and is waiting on a soak before it
# becomes the default. Until this lane existed its 32-bit arm had never run
# anywhere: 64-bit hosts always take the table, and the pre-commit legs only
# compile it. callbacktest is the app for it — widget creation, listener
# churn and deletes — with the sanitizer live, which on the default 32-bit
# path has nothing to check.
if [[ "$ARCH" == "arm32" && -z "$SPECIFIC_APP" ]]; then
  ht32_tag="callbacktest[no-shrink/arm32-ht32]"
  ht32_log="$RUN_LOG_DIR/callbacktest.no-shrink-arm32-ht32.log"
  ht32_build_log="$RUN_LOG_DIR/callbacktest.no-shrink-arm32-ht32.build.log"
  # Take the expected patterns from the conf rather than a second copy here.
  # callbacktest is a `loop` app: it never exits on its own, so timeout's 124
  # is the success path and the patterns are the actual assertion.
  ht32_patterns="$(awk -F'|' '$1 == "callbacktest" { print $4; exit }' "$SIM_CONF")"
  ht32_timeout=$(( $(awk -F'|' '$1 == "callbacktest" { print $3; exit }' "$SIM_CONF") * ARM32_TIMEOUT_MULT ))
  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $ht32_tag (handle-table-32, ${ht32_timeout}s) ---"
  if bash "$SCRIPT_DIR/build-apk.sh" --app callbacktest > "$ht32_build_log" 2>&1 &&
     env PICODROID_APK_PATH="sim-runtime" cargo build -p picodroid --release \
       --target "$(sim_target arm32)" --no-default-features \
       --features "sim,board-testbench-rp2350,handle-table-32" \
       >> "$ht32_build_log" 2>&1; then
    env PICODROID_APK_PATH="$REPO_ROOT/build/apks/callbacktest.papk" \
        PICODROID_SIM_HEADLESS=1 PICODROID_HANDLE_SANITIZER=1 \
        PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
        PICODROID_SIM_FS="$REPO_ROOT/platforms/rp/target/sim-fs.arm32-ht32.img" \
        timeout "$ht32_timeout" \
        qemu-arm-static -L /usr/arm-linux-gnueabihf \
        "$REPO_ROOT/target/$(sim_target arm32)/release/picodroid" \
        > "$ht32_log" 2>&1 < /dev/null || true
    if check_patterns "$ht32_log" "$ht32_patterns" > /dev/null 2>&1 \
       && check_no_crash "$ht32_log" > /dev/null 2>&1; then
      sim_log "  PASS"
      echo "PASS $ht32_tag" >> "$RESULTS_FILE"
      PASS=$((PASS + 1))
    else
      sim_log "  FAIL"
      tail -10 "$ht32_log" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
      check_patterns "$ht32_log" "$ht32_patterns" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
      check_no_crash "$ht32_log" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
      echo "FAIL $ht32_tag" >> "$RESULTS_FILE"
      FAIL=$((FAIL + 1))
    fi
  else
    sim_log "  BUILD FAILED (handle-table-32)"
    echo "ERROR $ht32_tag (build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
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
