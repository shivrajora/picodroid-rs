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

# net rows start an echo + HTTP server on this host; make sure they are gone
# on every exit path.
trap 'stop_net_listeners' EXIT

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

  # Build APK, into a per-mode path rather than the shared
  # build/apks/<app>.papk. Both shrink modes used to write that one file, so a
  # concurrent sim.sh or pre-commit run on the same checkout could swap a
  # shrunk papk under an unshrunk sim binary -- verify_compat then rejects it
  # at load with FrameworkVersionMismatch, which reads as a code regression
  # rather than two builds colliding.
  sim_log "  Building APK..."
  local apk_path="$REPO_ROOT/build/apks/sim-run/${mode}/${app}.papk"
  local -a apk_args=(--app "$app" -o "$apk_path" --board "$board")
  [[ "$mode" == "shrink" ]] && apk_args+=(--shrink)
  # net rows talk to 127.0.0.1 (the NetTestConfig.HOST default; sim sockets
  # are host sockets). Drop any inherited test-host override so a stale
  # export cannot point the sim elsewhere.
  if ! env -u PICODROID_NET_TEST_HOST bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (APK)"
    echo "ERROR $tag (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # Build sim binary (release). PICODROID_SHRINK must match the APK's mode
  # or verify_compat will reject at load time.
  #
  # The build-time PICODROID_APK_PATH is a constant marker, not the real
  # path: sim binaries load the .papk at startup from the *runtime* env var
  # (picodroid-core's hal/sim/app_region.rs), and the framework-class embed
  # only keys on the var being set. A stable value means the first build per
  # mode is the only real build — switching apps is a cargo no-op.
  sim_log "  Building sim binary..."
  local -a cargo_env=(PICODROID_APK_PATH="sim-runtime")
  [[ "$mode" == "shrink" ]] && cargo_env+=(PICODROID_SHRINK=1)
  if ! env "${cargo_env[@]}" cargo build \
    --release \
    --target "$HOST_TARGET" \
    --no-default-features \
    --features "sim,$board_feature,line-numbers" >> "$build_log" 2>&1; then
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
# Args: mode, app, lane name (log stem + bench-backfill BOARD_BY_APP key), log
# tag. The defaults are the Java app; picoenvmon_kt (its Kotlin twin, roadmap
# Session 7) rides the same two lanes with its own names.
run_enviro_smoke() {
  local mode="$1"
  local app="${2:-picoenvmon}"
  local lane="${3:-picoenvmon-enviro}"
  local logtag="${4:-PicoEnvMon}"
  local tag="${lane}[${mode}]"
  local log_file="$RUN_LOG_DIR/${lane}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${lane}.${mode}.build.log"
  local patterns="${logtag}[]:] Home.onCreate"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag (board smoke, 25s) ---"

  # Per-mode path -- see the note in run_test.
  local apk_path="$REPO_ROOT/build/apks/sim-run/${mode}/${app}.papk"
  local -a apk_args=(--app "$app" -o "$apk_path" --board pico_enviro_mon)
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
    --features "sim,board-pico-enviro-mon,line-numbers" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim, enviro board)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  PICODROID_APK_PATH="$apk_path" \
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

# Multi-app smoke (docs/designs/multi-app-2026-09.md M2): the launcher that
# every multi-app firmware links in, driven over the control FIFO the way
# the bench drives it over pdb. Boot into the launcher with helloworld
# installed, tap row 0, expect helloworld to run and the launcher to come
# back, then reinstall and uninstall helloworld through the package verbs
# while the launcher runs.
run_launcher_smoke() {
  local mode="$1"
  local app=helloworld lane=launcher
  local tag="${lane}[${mode}]"
  local log_file="$RUN_LOG_DIR/${lane}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${lane}.${mode}.build.log"
  local patterns="Launcher[]:] ready: 1 apps;Launcher[]:] launch helloworld;HelloWorld[]:] hi;apps: installed helloworld;apps: uninstalled helloworld;apps: \(none installed\)"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag (launcher smoke, 90s) ---"

  local apk_path="$REPO_ROOT/build/apks/sim-run/${mode}/${app}.papk"
  local launcher_path="$REPO_ROOT/build/apks/sim-run/${mode}/launcher.papk"
  local -a apk_args=(--app "$app" -o "$apk_path" --board testbench_rp2350)
  local -a launcher_args=(--app launcher -o "$launcher_path" --board testbench_rp2350)
  if [[ "$mode" == "shrink" ]]; then
    apk_args+=(--shrink)
    launcher_args+=(--shrink)
  fi
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1 \
     || ! bash "$SCRIPT_DIR/build-apk.sh" "${launcher_args[@]}" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (APK)"
    echo "ERROR $tag (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # The same binary run_test built for this mode (a cargo no-op).
  local -a cargo_env=(PICODROID_APK_PATH="sim-runtime")
  [[ "$mode" == "shrink" ]] && cargo_env+=(PICODROID_SHRINK=1)
  if ! env "${cargo_env[@]}" cargo build \
    --release \
    --target "$HOST_TARGET" \
    --no-default-features \
    --features "sim,board-testbench-rp2350,line-numbers" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  local fifo="$RUN_LOG_DIR/${lane}.${mode}.fifo"
  rm -f "$fifo"
  mkfifo "$fifo"
  PICODROID_APK_PATH="$apk_path" \
    PICODROID_SYSTEM_APKS="$launcher_path" \
    PICODROID_BOOT=launcher \
    PICODROID_SIM_CTRL_FIFO="$fifo" \
    PICODROID_SIM_HEADLESS=1 \
    PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
    PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
    timeout 90 "$bin" > "$log_file" 2>&1 < /dev/null &
  local pid=$!

  # Wait for `want` to appear `n` times in the log, up to 40 s.
  launcher_wait() {
    local want="$1" n="$2" i
    for i in $(seq 1 40); do
      [[ "$(grep -c -- "$want" "$log_file")" -ge "$n" ]] && return 0
      kill -0 "$pid" 2>/dev/null || return 1
      sleep 1
    done
    return 1
  }
  launcher_send() { printf '%s\n' "$1" > "$fifo"; }

  if launcher_wait "\[Launcher\] ready" 1; then
    launcher_send "input tap 120 20"
    if launcher_wait "\[Launcher\] ready" 2; then
      launcher_send "apps install $apk_path"
      if launcher_wait "apps: installed" 1; then
        launcher_send "apps uninstall $app"
        launcher_wait "apps: uninstalled" 1 || true
        sleep 1
      fi
    fi
  fi
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  rm -f "$fifo"

  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1 \
     && check_no_crash "$log_file" > /dev/null 2>&1; then
    sim_log "  PASS"
    echo "PASS $tag" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL"
    tail -8 "$log_file" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
    echo "FAIL $tag" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}

# The bridge lane: the simulator as a `pdb` device
# (docs/designs/sim-pdb-endpoint-2026-09.md). Boot the launcher with
# helloworld installed and drive it with the real `pdb` binary over the
# simulator's socket — the conf's `pdb` rows, on the host: ping, list, sysmon,
# an input tap that launches helloworld, an install that reboots the
# simulator (exec, warm boot, the launcher back with one more app), a refused
# install the launcher survives (the device-side compat check, after the
# park), an uninstall that reboots again, and a ping of the rebooted process.
run_pdb_smoke() {
  local mode="$1"
  local app=helloworld second=blinky lane=pdb
  local tag="${lane}[${mode}]"
  local log_file="$RUN_LOG_DIR/${lane}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${lane}.${mode}.build.log"
  local patterns="\[sim\] pdb: listening on;Launcher[]:] ready: 1 apps;HelloWorld[]:] hi;\[sim\] reboot: requested by the debug bridge;\[sim\] apps: warm boot from;Launcher[]:] ready: 2 apps"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag (pdb bridge smoke, 150s) ---"

  # Four PAPKs: the boot app, the launcher, a second app to install and
  # remove, and the boot app built in the *other* shrink mode — a
  # framework-map-version the simulator must refuse after parking.
  local apk_dir="$REPO_ROOT/build/apks/sim-run/${mode}"
  local apk_path="$apk_dir/${app}.papk"
  local launcher_path="$apk_dir/launcher.papk"
  local second_path="$apk_dir/${second}.papk"
  local reject_path="$apk_dir/${app}-other-mode.papk"
  local -a this_mode=() other_mode=(--shrink)
  if [[ "$mode" == "shrink" ]]; then
    this_mode=(--shrink)
    other_mode=()
  fi
  local -a build=(bash "$SCRIPT_DIR/build-apk.sh" --board testbench_rp2350)
  if ! "${build[@]}" --app "$app" -o "$apk_path" ${this_mode[@]+"${this_mode[@]}"} > "$build_log" 2>&1 \
     || ! "${build[@]}" --app launcher -o "$launcher_path" ${this_mode[@]+"${this_mode[@]}"} >> "$build_log" 2>&1 \
     || ! "${build[@]}" --app "$second" -o "$second_path" ${this_mode[@]+"${this_mode[@]}"} >> "$build_log" 2>&1 \
     || ! "${build[@]}" --app "$app" -o "$reject_path" ${other_mode[@]+"${other_mode[@]}"} >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (APK)"
    echo "ERROR $tag (apk build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  # The same binary run_test built for this mode (a cargo no-op), and the
  # host tool.
  local -a cargo_env=(PICODROID_APK_PATH="sim-runtime")
  [[ "$mode" == "shrink" ]] && cargo_env+=(PICODROID_SHRINK=1)
  if ! env "${cargo_env[@]}" cargo build \
    --release \
    --target "$HOST_TARGET" \
    --no-default-features \
    --features "sim,board-testbench-rp2350,line-numbers" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi
  if ! cargo build --release --target "$HOST_TARGET" \
    --manifest-path "$REPO_ROOT/tools/pdb/Cargo.toml" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (pdb)"
    echo "ERROR $tag (pdb build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  local pdb="$REPO_ROOT/target/$HOST_TARGET/release/pdb"
  # A short socket path (Unix sockets cap the path at ~100 bytes) of this
  # lane's own, so a developer's simulator on the same host is never picked.
  local sockdir
  sockdir="$(mktemp -d /tmp/picodroid-pdb-lane.XXXXXX)"
  local sock="$sockdir/pdb.sock"
  local fs_img="$RUN_LOG_DIR/${lane}.${mode}.fs.img"
  rm -f "$fs_img"
  PICODROID_APK_PATH="$apk_path" \
    PICODROID_SYSTEM_APKS="$launcher_path" \
    PICODROID_BOOT=launcher \
    PICODROID_SIM_PDB_SOCKET="$sock" \
    PICODROID_SIM_FS="$fs_img" \
    PICODROID_SIM_HEADLESS=1 \
    PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
    PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
    timeout 150 "$bin" > "$log_file" 2>&1 < /dev/null &
  local pid=$!

  # Wait for `want` to appear `n` times in the sim log, up to 40 s. The
  # process id is the same across the simulator's exec reboots.
  pdb_wait() {
    local want="$1" n="$2" i
    for i in $(seq 1 40); do
      [[ "$(grep -c -- "$want" "$log_file")" -ge "$n" ]] && return 0
      kill -0 "$pid" 2>/dev/null || return 1
      sleep 1
    done
    return 1
  }
  # Run one pdb command against the socket; its output goes to a per-step
  # log next to the sim's. `-k 5`: an install waits up to two minutes for
  # READY, and a pdb wedged on a vanished socket ignores TERM.
  pdb_run() {
    local step="$1"
    shift
    timeout -k 5 130 "$pdb" -s "$sock" "$@" > "$RUN_LOG_DIR/${lane}.${mode}.pdb-${step}.log" 2>&1
  }
  pdb_said() { grep -qE -- "$2" "$RUN_LOG_DIR/${lane}.${mode}.pdb-${1}.log"; }

  local failed=""
  if ! pdb_wait "\[sim\] pdb: listening on" 1 || ! pdb_wait "\[Launcher\] ready: 1 apps" 1; then
    failed="the launcher did not come up"
  elif ! pdb_run ping ping || ! pdb_said ping "picodroid/2\." || ! pdb_said ping "apps 1/"; then
    failed="ping"
  elif ! pdb_run list list || ! pdb_said list "^[0-9]+ +$app " || ! pdb_said list "^SYSTEM +picodroid.launcher " || ! pdb_said list "^running: picodroid.launcher"; then
    failed="list"
  elif ! pdb_run sysmon sysmon || ! pdb_said sysmon "^Uptime:" || ! pdb_said sysmon " jvm " || ! pdb_said sysmon " pdb "; then
    failed="sysmon"
  elif ! pdb_run tap input tap 120 20 || ! pdb_wait "\[HelloWorld\] hi" 1 || ! pdb_wait "\[Launcher\] ready" 2; then
    failed="input tap (helloworld from the launcher's first row)"
  elif ! pdb_run install install "$second_path" || ! pdb_said install "^Install complete\."; then
    failed="install"
  elif ! pdb_wait "\[sim\] reboot: requested" 1 || ! pdb_wait "\[sim\] apps: warm boot from" 1 || ! pdb_wait "\[Launcher\] ready: 2 apps" 1; then
    failed="reboot after the install"
  elif ! pdb_run list2 list || ! pdb_said list2 "^[0-9]+ +$second "; then
    failed="list after the install"
  elif ! pdb_run reject install --skip-host-check --expect-rejected "$reject_path" || ! pdb_said reject "STATUS_INCOMPAT"; then
    failed="device-side reject of the other-mode PAPK"
  elif ! pdb_wait "\[Launcher\] ready: 2 apps" 2; then
    failed="the launcher did not come back after the refused install"
  elif ! pdb_run uninstall uninstall "$second" || ! pdb_said uninstall "^Device is back\."; then
    failed="uninstall"
  elif ! pdb_wait "\[sim\] reboot: requested" 2 || ! pdb_wait "\[Launcher\] ready: 1 apps" 2; then
    failed="reboot after the uninstall"
  elif ! pdb_run ping2 ping || ! pdb_said ping2 "apps 1/"; then
    failed="ping after two reboots"
  fi
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  rm -rf "$sockdir"

  if [[ -z "$failed" ]] \
     && check_patterns "$log_file" "$patterns" > /dev/null 2>&1 \
     && check_no_crash "$log_file" > /dev/null 2>&1; then
    sim_log "  PASS"
    echo "PASS $tag" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL${failed:+ ($failed)}"
    tail -8 "$log_file" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
    check_no_crash "$log_file" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
    echo "FAIL $tag${failed:+ ($failed)}" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}

# The alarm lane: the point of AlarmManager, which no single-app run can show.
# alarmdemo arms an alarm, walks out to the launcher, and the framework starts
# it again to deliver it. Everything here happens on its own — the only reason
# the control FIFO exists is to tap the launcher's first row.
run_alarm_smoke() {
  local mode="$1"
  local app=alarmdemo lane=alarm
  local tag="${lane}[${mode}]"
  local log_file="$RUN_LOG_DIR/${lane}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${lane}.${mode}.build.log"
  local patterns="AlarmDemo[]:] armed id=7;AlarmDemo[]:] leaving for the launcher;[[]alarm[]] wake alarmdemo;[[]alarm[]] fire alarmdemo;AlarmDemo[]:] woke id=7"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag (alarm wake path, 90s) ---"

  local apk_path="$REPO_ROOT/build/apks/sim-run/${mode}/${app}.papk"
  local launcher_path="$REPO_ROOT/build/apks/sim-run/${mode}/launcher.papk"
  local -a apk_args=(--app "$app" -o "$apk_path" --board testbench_rp2350)
  local -a launcher_args=(--app launcher -o "$launcher_path" --board testbench_rp2350)
  if [[ "$mode" == "shrink" ]]; then
    apk_args+=(--shrink)
    launcher_args+=(--shrink)
  fi
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1 \
     || ! bash "$SCRIPT_DIR/build-apk.sh" "${launcher_args[@]}" >> "$build_log" 2>&1; then
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
    --features "sim,board-testbench-rp2350,line-numbers" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  local fifo="$RUN_LOG_DIR/${lane}.${mode}.fifo"
  rm -f "$fifo"
  mkfifo "$fifo"
  # Boot the launcher, not the app: the demo has to be started from outside
  # for leaving it to mean anything.
  PICODROID_APK_PATH="$apk_path" \
    PICODROID_SYSTEM_APKS="$launcher_path" \
    PICODROID_BOOT=launcher \
    PICODROID_SIM_CTRL_FIFO="$fifo" \
    PICODROID_SIM_HEADLESS=1 \
    PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
    PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
    timeout 90 "$bin" > "$log_file" 2>&1 < /dev/null &
  local pid=$!

  alarm_wait() {
    local want="$1" n="$2" i
    for i in $(seq 1 40); do
      [[ "$(grep -c -- "$want" "$log_file")" -ge "$n" ]] && return 0
      kill -0 "$pid" 2>/dev/null || return 1
      sleep 1
    done
    return 1
  }

  if alarm_wait "\[Launcher\] ready" 1; then
    printf '%s\n' "input tap 120 20" > "$fifo"
    # Arm, leave, wake, deliver: about five seconds of it, and then the
    # demo finishes and the launcher comes back.
    alarm_wait "woke id=7" 1 || true
    sleep 1
  fi
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  rm -f "$fifo"

  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1 \
     && check_no_crash "$log_file" > /dev/null 2>&1; then
    sim_log "  PASS"
    echo "PASS $tag" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL"
    tail -8 "$log_file" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
    echo "FAIL $tag" >> "$RESULTS_FILE"
    FAIL=$((FAIL + 1))
  fi
}

# The settings lane (multi-app M3): the launcher with helloworld and the
# settings app installed, driven over the control FIFO through About,
# Storage and Apps, where helloworld is uninstalled through the dialog and
# the directory ends up empty; Home returns to the launcher. The dialog's
# Uninstall button sits at DIALOG_OK (lib.sh::settings_dialog_ok: the card
# is centred on the board's display; the device draws the same card).
run_settings_smoke() {
  local DIALOG_OK="${PICODROID_SETTINGS_DIALOG_OK:-$(settings_dialog_ok testbench_rp2350)}"
  local mode="$1"
  local app=helloworld lane=settings
  local tag="${lane}[${mode}]"
  local log_file="$RUN_LOG_DIR/${lane}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${lane}.${mode}.build.log"
  local patterns="Launcher[]:] ready: 2 apps;Settings[]:] ready;Settings[]:] about;Settings[]:] storage helloworld;Settings[]:] apps 1;Settings[]:] uninstalled helloworld;Settings[]:] apps 0;apps: \(none installed\);Launcher[]:] ready: 1 apps"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag (settings smoke, 120s) ---"

  local apk_path="$REPO_ROOT/build/apks/sim-run/${mode}/${app}.papk"
  local launcher_path="$REPO_ROOT/build/apks/sim-run/${mode}/launcher.papk"
  local settings_path="$REPO_ROOT/build/apks/sim-run/${mode}/settings.papk"
  local -a apk_args=(--app "$app" -o "$apk_path" --board testbench_rp2350)
  local -a launcher_args=(--app launcher -o "$launcher_path" --board testbench_rp2350)
  local -a settings_args=(--app settings -o "$settings_path" --board testbench_rp2350)
  if [[ "$mode" == "shrink" ]]; then
    apk_args+=(--shrink)
    launcher_args+=(--shrink)
    settings_args+=(--shrink)
  fi
  if ! bash "$SCRIPT_DIR/build-apk.sh" "${apk_args[@]}" > "$build_log" 2>&1 \
     || ! bash "$SCRIPT_DIR/build-apk.sh" "${launcher_args[@]}" >> "$build_log" 2>&1 \
     || ! bash "$SCRIPT_DIR/build-apk.sh" "${settings_args[@]}" >> "$build_log" 2>&1; then
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
    --features "sim,board-testbench-rp2350,line-numbers" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  local fifo="$RUN_LOG_DIR/${lane}.${mode}.fifo"
  rm -f "$fifo"
  mkfifo "$fifo"
  PICODROID_APK_PATH="$apk_path" \
    PICODROID_SYSTEM_APKS="$launcher_path:$settings_path" \
    PICODROID_BOOT=launcher \
    PICODROID_SIM_CTRL_FIFO="$fifo" \
    PICODROID_SIM_HEADLESS=1 \
    PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
    PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
    timeout 120 "$bin" > "$log_file" 2>&1 < /dev/null &
  local pid=$!

  settings_wait() {
    local want="$1" n="$2" i
    for i in $(seq 1 40); do
      [[ "$(grep -c -- "$want" "$log_file")" -ge "$n" ]] && return 0
      kill -0 "$pid" 2>/dev/null || return 1
      sleep 1
    done
    return 1
  }
  settings_send() { printf '%s\n' "$1" > "$fifo"; sleep 1; }

  # Rows are 40 px: the launcher lists helloworld (row 0) then Settings
  # (row 1); the settings screens put their header at row 0.
  if settings_wait "\[Launcher\] ready: 2 apps" 1; then
    settings_send "input tap 120 60"                       # Settings
    if settings_wait "\[Settings\] ready" 1; then
      settings_send "input tap 120 60"                     # About
      settings_wait "\[Settings\] about" 1 || true
      settings_send "input tap 120 20"                     # back
      settings_wait "\[Settings\] ready" 2 || true
      settings_send "input tap 120 140"                    # Storage
      settings_wait "\[Settings\] storage helloworld" 1 || true
      settings_send "input tap 120 20"                     # back
      settings_wait "\[Settings\] ready" 3 || true
      settings_send "input tap 120 100"                    # Apps
      if settings_wait "\[Settings\] apps 1" 1; then
        settings_send "input tap 120 60"                   # helloworld → the dialog
        settings_send "input tap $DIALOG_OK"               # Uninstall
        settings_wait "\[Settings\] apps 0" 1 || true
        settings_send "apps list"
        settings_wait "apps: (none installed)" 1 || true
      fi
      settings_send "input tap 120 20"                     # back to the root
      settings_wait "\[Settings\] ready" 4 || true
      settings_send "input tap 120 20"                     # Home
      settings_wait "\[Launcher\] ready: 1 apps" 1 || true
    fi
  fi
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  rm -f "$fifo"

  if check_patterns "$log_file" "$patterns" > /dev/null 2>&1 \
     && check_no_crash "$log_file" > /dev/null 2>&1; then
    sim_log "  PASS"
    echo "PASS $tag" >> "$RESULTS_FILE"
    PASS=$((PASS + 1))
  else
    sim_log "  FAIL"
    tail -8 "$log_file" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
    check_patterns "$log_file" "$patterns" 2>&1 | while IFS= read -r line; do sim_log "  $line"; done || true
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
  local app="${2:-picoenvmon}"
  local lane="${3:-picoenvmon-enviro-w}"
  local logtag="${4:-PicoEnvMon}"
  local tag="${lane}[${mode}]"
  local log_file="$RUN_LOG_DIR/${lane}.${mode}.log"
  local build_log="$RUN_LOG_DIR/${lane}.${mode}.build.log"
  local patterns="${logtag}[]:] Home.onCreate;net: up;http: serving on port 8080"

  TOTAL=$((TOTAL + 1))
  sim_log "--- [$TOTAL] $tag (WiFi board smoke, 25s) ---"

  # Per-mode path -- see the note in run_test.
  local apk_path="$REPO_ROOT/build/apks/sim-run/${mode}/${app}.papk"
  local -a apk_args=(--app "$app" -o "$apk_path" --board pico_enviro_mon_w)
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
    --features "sim,board-pico-enviro-mon-w,line-numbers" >> "$build_log" 2>&1; then
    sim_log "  BUILD FAILED (sim, enviro-w board)"
    echo "ERROR $tag (sim build failed)" >> "$RESULTS_FILE"
    ERROR=$((ERROR + 1))
    return
  fi

  local bin="$REPO_ROOT/target/$HOST_TARGET/release/picodroid"
  PICODROID_APK_PATH="$apk_path" \
    PICODROID_SIM_HEADLESS=1 \
    PICODROID_HANDLE_SANITIZER="${PICODROID_HANDLE_SANITIZER:-1}" \
    PICODROID_PARITY_STRICT="${PICODROID_PARITY_STRICT:-1}" \
    timeout 25 "$bin" > "$log_file" 2>&1 < /dev/null &
  local sim_pid=$!

  # Probe the dashboard once the server line appears (bounded wait). NTP +
  # weather housekeeping runs on the background pool, off the serve thread, so
  # an unanswered page is a finding, not expected (it was until 2026-09-04: the
  # 2026-08-18 nightly failed on a ~11 s housekeeping stall). The short retry
  # only covers the moment right after "http: serving" appears.
  local page_ok=0
  local i attempt
  for i in $(seq 1 20); do
    if grep -q "http: serving" "$log_file" 2>/dev/null; then
      for attempt in 1 2 3; do
        if curl -sf -m 5 "http://127.0.0.1:8080/" 2>/dev/null | grep -q "$logtag"; then
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
  # rows, an optional board override for sim rows (e.g. netexception needs
  # the network-enabled W board's sim build) and the required board for net
  # rows.
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

    # Skip the conf's pdb rows: their steps are hil-run.sh's, written for a
    # flashed board. The simulator's bridge is exercised by `run_pdb_smoke`
    # below, with the same host tool over the simulator's socket.
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

    # net rows need the host-side echo (7000) and HTTP (8000) servers.
    if [[ "$category" == "net" ]]; then
      if ! start_net_listeners "$RUN_LOG_DIR"; then
        sim_log "ERROR $app[$MODE] ($NET_LISTENER_ERR)"
        echo "ERROR $app[$MODE] (listeners)" >> "$RESULTS_FILE"
        ERROR=$((ERROR + 1))
        TOTAL=$((TOTAL + 1))
        continue
      fi
    fi

    if [[ ( "$category" == "sim" || "$category" == "net" ) && -n "${extra:-}" ]]; then
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

  # Scheduling-diagnostics soak (docs/scheduling-diagnostics.md): strict
  # monitor over the tick loop, Java threads and the clock app, plus the
  # detector self-test. Shrink-invariant like mem-diag: once per cycle.
  if [[ -z "$SPECIFIC_APP" && "$MODE" != "shrink" ]]; then
    TOTAL=$((TOTAL + 1))
    sim_log "--- [$TOTAL] sched-diag soak ---"
    scheddiag_log="$RUN_LOG_DIR/sched-diag.log"
    if bash "$SCRIPT_DIR/test-scheddiag.sh" > "$scheddiag_log" 2>&1; then
      sim_log "  PASS"
      echo "PASS sched-diag" >> "$RESULTS_FILE"
      PASS=$((PASS + 1))
    else
      sim_log "  FAIL"
      tail -10 "$scheddiag_log" 2>/dev/null | while IFS= read -r line; do sim_log "    $line"; done || true
      echo "FAIL sched-diag" >> "$RESULTS_FILE"
      FAIL=$((FAIL + 1))
    fi
  fi

  # Enviro-board smoke: full runs and `--app picoenvmon` (the CI hook; the
  # conf matrix has no picoenvmon row, so that invocation reaches only this).
  if [[ -z "$SPECIFIC_APP" || "$SPECIFIC_APP" == "picoenvmon" ]]; then
    run_enviro_smoke "$MODE"
    run_enviro_w_smoke "$MODE"
  fi
  # The launcher lane (multi-app M2); `--app launcher` reaches only this.
  if [[ -z "$SPECIFIC_APP" || "$SPECIFIC_APP" == "launcher" ]]; then
    run_launcher_smoke "$MODE"
  fi
  # The settings lane (multi-app M3); `--app settings` reaches only this.
  if [[ -z "$SPECIFIC_APP" || "$SPECIFIC_APP" == "settings" ]]; then
    run_settings_smoke "$MODE"
  fi
  # The bridge lane: the real pdb tool against the simulator's socket;
  # `--app pdb` reaches only this.
  if [[ -z "$SPECIFIC_APP" || "$SPECIFIC_APP" == "pdb" ]]; then
    run_pdb_smoke "$MODE"
  fi
  # The alarm lane: an alarm outliving the app that set it, which the
  # conf's `alarmdemo` row cannot show on its own (it runs one app, with
  # no launcher to leave for). `--app alarmdemo` reaches both.
  if [[ -z "$SPECIFIC_APP" || "$SPECIFIC_APP" == "alarmdemo" ]]; then
    run_alarm_smoke "$MODE"
  fi
  # The Kotlin twin (examples/picoenvmon_kt): same boards, same proofs, its own
  # log tag and lane names (docs/designs/kotlin-roadmap-2026-08.md Session 7).
  if [[ -z "$SPECIFIC_APP" || "$SPECIFIC_APP" == "picoenvmon_kt" ]]; then
    run_enviro_smoke "$MODE" picoenvmon_kt picoenvmon_kt-enviro PicoEnvMonKt
    run_enviro_w_smoke "$MODE" picoenvmon_kt picoenvmon_kt-enviro-w PicoEnvMonKt
  fi
done

stop_net_listeners

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
