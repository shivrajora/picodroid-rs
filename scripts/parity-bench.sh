#!/usr/bin/env bash
# Parity benchmark runner (docs/parity-audit.md P2).
#
# Runs the bench apps with `parity-metrics` enabled on the simulator
# (release, headless) and/or a HIL board, extracts wall-clock plus the
# deterministic counters, and appends rows to bench/parity/history.csv:
#
#   utc,commit,env,app,metric,value
#
# Interpretation contract:
#   - insns/allocs/gcs/bands/fbytes are deterministic work counters and must
#     be EQUAL between the sim and hil rows of the same commit+app; any
#     inequality is a runtime divergence (memory, threading, dispatch), not
#     a performance signal.
#   - wall_ms never adjudicates a change across a rebuild. The XIP/icache
#     layout re-rolls on every link: measured sigma ~4% on aggregate device
#     wall_ms and up to +/-40% per microbenchmark, against a 32 ppm floor for
#     repeated runs of one flashed image (docs/perf-campaign-2026-08.md S1.3).
#     Use --runs to sample one image; use scripts/bench-report.py to gate.
#     A host wall-clock number never predicts device wall-clock.
#
# Usage:
#   ./scripts/parity-bench.sh                     # sim lane, all bench apps
#   ./scripts/parity-bench.sh --apps benchmark,perfbench
#   ./scripts/parity-bench.sh --runs 3            # N samples, emitted as N rows
#   ./scripts/parity-bench.sh --size-only --boards testbench_rp2040,testbench_rp2350
#   ./scripts/parity-bench.sh --hil               # HIL lane (board + probe attached)
#   ./scripts/parity-bench.sh --check             # ratio-drift check over the CSV
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

CSV="$REPO_ROOT/bench/parity/history.csv"
BOARD="testbench_rp2350"
APPS=(benchmark perfbench graphicsbench)
DO_SIM=true
DO_HIL=false
DO_CHECK=false
DO_SIZE=false
APPS_SET=false
RUNS=""
BOARDS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)   [[ "$APPS_SET" == true ]] || APPS=(); APPS_SET=true
             APPS+=("$2"); shift 2 ;;
    --apps)  APPS_SET=true; IFS=, read -r -a APPS <<< "$2"; shift 2 ;;
    --board) BOARD="$2"; shift 2 ;;
    --boards) IFS=, read -r -a BOARDS <<< "$2"; shift 2 ;;
    --runs)  RUNS="$2"; shift 2 ;;
    --size-only) DO_SIZE=true; DO_SIM=false; shift ;;
    --hil)   DO_HIL=true; DO_SIM=false; shift ;;
    --both)  DO_HIL=true; DO_SIM=true; shift ;;
    --check) DO_CHECK=true; DO_SIM=false; shift ;;
    -h|--help) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

mkdir -p "$(dirname "$CSV")"
[[ -f "$CSV" ]] || echo "utc,commit,env,board,app,mode,split,metric,value" > "$CSV"

# Apps whose numbers may drive accept/reject; everything else may only veto.
is_train() { [[ "$1" == benchmark || "$1" == perfbench ]]; }

# UTC is re-stamped per emitted run: the CSV primary key is every column but
# the value, so a fixed timestamp would make N runs of the same app collapse
# into one row instead of N samples.
UTC="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
# A dirty tree gets its own identity. Tagging uncommitted work with the plain
# HEAD hash makes before-and-after rows collide on the CSV primary key, so a
# comparison silently reads its own "after" numbers as the baseline and reports
# everything identical. The diff digest keeps successive working states
# distinct, which is what an inner measurement loop needs.
COMMIT="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
if ! git -C "$REPO_ROOT" diff --quiet HEAD 2>/dev/null; then
  COMMIT="${COMMIT}-d$(git -C "$REPO_ROOT" diff HEAD | sha1sum | cut -c1-6)"
fi
stamp_utc() { UTC="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"; }
HOST_TARGET="$(host_target)"

# Reports the spread of a batch, and flags a batch that is too noisy to use.
#
# The threshold is per-environment and the difference is large: repeated runs
# of one flashed image are ~32 ppm reproducible, so a device batch spreading
# more than 0.5% means the board is in a bad state (thermal / USB / probe) and
# should be discarded rather than averaged in. The sim has a 4-5% same-binary
# spread by nature, so the same threshold there is pure false alarm -- it is
# reported but never flagged. (docs/perf-campaign-2026-08.md S1.3)
report_spread() { # app limit_percent  < values-on-stdin
  awk -v app="$1" -v limit="$2" '
    /^[0-9]+$/ { v[n++] = $1; if (mn == "" || $1 < mn) mn = $1; if ($1 > mx) mx = $1 }
    END {
      if (n == 0) { printf "  %s: no wall-clock captured\n", app; exit }
      if (n == 1) { printf "  %s: 1 run, %d ms\n", app, mn; exit }
      p = (mx - mn) / mn * 100
      printf "  %s: %d runs, %d..%d ms, p2p %.3f%%%s\n", app, n, mn, mx, p,
             (limit > 0 && p > limit ? "  <== BAD BATCH, board unstable, re-run" : "")
    }'
}

emit() { # env app metric value
  local split=holdout
  is_train "$2" && split=train
  local mode=no-shrink
  [[ "${PICODROID_SHRINK:-0}" == "1" ]] && mode=shrink
  echo "$UTC,$COMMIT,$1,${EMIT_BOARD:-$BOARD},$2,$mode,$split,$3,$4" >> "$CSV"
}

# Parsing is delegated to scripts/bench-backfill.py so this script and the
# nightly backfill cannot drift apart. Running both parsers over the same
# workload used to produce disjoint metric sets -- this one emitted four
# metrics, the backfill emitted ~38 -- which made a parity-bench run
# incomparable with the nightly corpus it is supposed to extend.
#
# Logs are written as <RUN_ID>/<app>.<mode>.log so the backfill can recover
# utc and commit from the directory name exactly as it does for a cron run.
# One run directory per sample. bench-backfill.py derives utc and commit from
# the directory name, and the CSV primary key is every column but the value --
# so N samples sharing one directory would collapse into one row instead of N.
new_run_dir() {
  local d="$LOG_DIR/$(date -u '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT}_$1"
  mkdir -p "$d"
  echo "$d"
}

ingest_run_dir() { # env board dir
  # A verification run measures but records nothing. Every commit runs the
  # size lane through the git hook, and those rows are by construction the
  # same numbers as the last commit's -- appending them only ever dirtied the
  # tree. PICODROID_BENCH_RECORD=0 turns the whole append off.
  if [[ "${PICODROID_BENCH_RECORD:-1}" != "1" ]]; then
    return 0
  fi
  python3 "$SCRIPT_DIR/bench-backfill.py" --run-dir "$3" \
    --force-env "$1" --board "$2" --out "$CSV" --quiet
}

# Retained only for the --check lane's legacy expectations.
parse_log() { # env app logfile
  local env_name="$1" app="$2" log="$3"
  local wall
  wall="$(grep -oE 'TOTAL: [0-9]+ ms' "$log" | tail -1 | grep -oE '[0-9]+' || true)"
  [[ -n "$wall" ]] && emit "$env_name" "$app" wall_ms "$wall"
  # gcstress/heapstress report microseconds; the original matcher only ever
  # looked for "ms" and silently dropped both.
  local wall_us
  wall_us="$(grep -oE 'TOTAL: [0-9]+ us' "$log" | tail -1 | grep -oE '[0-9]+' || true)"
  [[ -n "$wall_us" ]] && emit "$env_name" "$app" wall_us "$wall_us"
  local score
  score="$(grep -oE 'SCORE[:=] ?[0-9]+' "$log" | tail -1 | grep -oE '[0-9]+' || true)"
  [[ -n "$score" ]] && emit "$env_name" "$app" score "$score"
  local parity_line
  parity_line="$(grep -oE 'parity: insns=[0-9]+ allocs=[0-9]+ gcs=[0-9]+ bands=[0-9]+ fbytes=[0-9]+' "$log" | tail -1 || true)"
  if [[ -n "$parity_line" ]]; then
    for kv in $parity_line; do
      [[ "$kv" == parity: ]] && continue
      emit "$env_name" "$app" "${kv%%=*}" "${kv##*=}"
    done
  else
    echo "WARN: no parity counter line for $app ($env_name) — build lacks parity-metrics?" >&2
  fi
}

# Size lane. Flash and RAM are the only campaign metrics with a literally zero
# noise floor -- the link is deterministic -- and the RP2040 program region is
# the binding budget (96% full, enforced today only by link failure). Cheapest
# and sharpest signal we have. (docs/perf-campaign-2026-08.md)
if $DO_SIZE; then
  [[ ${#BOARDS[@]} -gt 0 ]] || BOARDS=("$BOARD")
  echo "==> Size lane (release firmware, ${#BOARDS[@]} board(s))"
  # Every other lane leaves a log that bench-backfill.py can re-read, so its
  # rows can always be rebuilt from build/*/logs. The size lane used to write
  # straight to the CSV, which made its rows the only copy -- and the only
  # rows in the file that a stray `git checkout` would destroy for good. It
  # now writes a log like everything else, and goes through the same parser.
  # A caller that wants to read the logs back (pre-commit, feeding
  # `bench-report.py --ratchet --sizes-from`) can name the directory.
  SIZE_RUN_DIR="${PICODROID_SIZE_RUN_DIR:-$REPO_ROOT/build/size/logs/$(date -u '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT}}"
  mkdir -p "$SIZE_RUN_DIR"
  for board in "${BOARDS[@]}"; do
    resolve_board "$board"
    APP="${SIZE_APP:-helloworld}"
    PROFILE=release
    EXTRA_ARGS=(--release)
    # Keep the build output: sent to /dev/null, a CI link failure surfaced as
    # a bare "exit code 1" under the lane header with nothing to read.
    build_log="$SIZE_RUN_DIR/$board.build.log"
    if ! build_firmware > "$build_log" 2>&1; then
      echo "  $board: BUILD FAILED"
      tail -30 "$build_log" | sed 's/^/    /'
      SIZE_FAILED=1
      continue
    fi
    # print_memory_usage exported TEXT/DATA/BSS; resolve_board exported the
    # ceilings. The log carries both, so headroom stays derivable later
    # without re-reading a linker script.
    flash=$(( TEXT + DATA ))
    ram=$(( DATA + BSS ))
    {
      # Several boards share one run directory, so the filename cannot encode
      # the identity; the log declares it instead.
      echo "#bench board=$board app=$APP mode=no-shrink"
      "$SIZE_TOOL" "$ELF"
      echo "#program_flash_max=$PROGRAM_FLASH_MAX"
      echo "#ram_max=$RAM_MAX"
    } > "$SIZE_RUN_DIR/$board.size.log"
    printf "  %-22s flash %8d / %8d (%2d%%, %6d free)   ram %7d / %7d (%2d%%, %6d free)\n" \
      "$board" "$flash" "$PROGRAM_FLASH_MAX" \
      "$(( flash * 100 / PROGRAM_FLASH_MAX ))" "$(( PROGRAM_FLASH_MAX - flash ))" \
      "$ram" "$RAM_MAX" "$(( ram * 100 / RAM_MAX ))" "$(( RAM_MAX - ram ))"
  done
  ingest_run_dir size "" "$SIZE_RUN_DIR"
  # A board that never built must not read as a clean lane -- the ratchet step
  # downstream would otherwise check a baseline nothing was measured against.
  if [[ -n "${SIZE_FAILED:-}" ]]; then
    echo "==> Size lane FAILED (see the build logs above)" >&2
    exit 1
  fi
fi

if $DO_SIM; then
  resolve_board "$BOARD"
  LOG_DIR="$(mktemp -d)"
  echo "==> Sim parity bench (board $BOARD, release, headless)"
  for app in "${APPS[@]}"; do
    bash "$SCRIPT_DIR/build-apk.sh" --app "$app" > /dev/null
    env PICODROID_APK_PATH="sim-runtime" cargo build --release -q \
      --manifest-path "$MANIFEST_DIR/Cargo.toml" -p "$PACKAGE" \
      --target "$HOST_TARGET" --no-default-features \
      --features "sim,$BOARD_FEATURE,parity-metrics"
    # Deterministic counters need exactly one run; sim wall-clock has a 4-5%
    # same-binary spread. N samples are emitted as N rows and never
    # pre-averaged -- averaging destroys the spread the analysis needs.
    n_runs="${RUNS:-1}"
    : > "$LOG_DIR/$app.sim.walls"
    mode_tag=no-shrink
    [[ "${PICODROID_SHRINK:-0}" == "1" ]] && mode_tag=shrink
    for run in $(seq 1 "$n_runs"); do
      run_dir="$(new_run_dir "$run")"
      local_log="$run_dir/$app.$mode_tag.log"
      # Bench apps terminate on their own; the timeout is a hang backstop.
      PICODROID_APK_PATH="$REPO_ROOT/build/apks/$app.papk" \
        PICODROID_SIM_HEADLESS=1 \
        timeout 600 "$REPO_ROOT/target/$HOST_TARGET/release/picodroid" \
        > "$local_log" 2>&1 || true
      ingest_run_dir sim "$BOARD" "$run_dir"
      grep -oE 'TOTAL: [0-9]+ ms' "$local_log" | grep -oE '[0-9]+' \
        >> "$LOG_DIR/$app.sim.walls" || true
    done
    report_spread "$app" 0 < "$LOG_DIR/$app.sim.walls"
  done
fi

if $DO_HIL; then
  echo "==> HIL parity bench (board $BOARD; flashes the attached device)"
  # One lease for the whole campaign; the flash.sh calls below re-acquire
  # idempotently under the same session identity.
  require_device_lock --hil --boards "$BOARD"
  LOG_DIR="${LOG_DIR:-$(mktemp -d)}"
  for app in "${APPS[@]}"; do
    # flash.sh stays attached streaming RTT; bench apps print their totals
    # within the capture window, then we cut the session.
    # PICODROID_EXTRA_FEATURES is appended to the firmware feature set by
    # lib.sh::build_firmware.
    #
    # N runs share ONE flashed image: device wall-clock is ~32 ppm reproducible
    # per image but re-rolls +/-4% on every rebuild, so re-flashing between
    # samples would measure the linker, not the change.
    n_runs="${RUNS:-3}"
    : > "$LOG_DIR/$app.hil.walls"
    mode_tag=no-shrink
    [[ "${PICODROID_SHRINK:-0}" == "1" ]] && mode_tag=shrink
    for run in $(seq 1 "$n_runs"); do
      run_dir="$(new_run_dir "$run")"
      local_log="$run_dir/$app.$mode_tag.log"
      PICODROID_EXTRA_FEATURES=parity-metrics \
        timeout 300 "$SCRIPT_DIR/flash.sh" -b "$BOARD" -a "$app" -r \
        > "$local_log" 2>&1 || true
      ingest_run_dir hil "$BOARD" "$run_dir"
      grep -oE 'TOTAL: [0-9]+ ms' "$local_log" | grep -oE '[0-9]+' \
        >> "$LOG_DIR/$app.hil.walls" || true
    done
    report_spread "$app" 0.5 < "$LOG_DIR/$app.hil.walls"
  done
fi

if $DO_CHECK; then
  # Two independent checks.
  #
  # 1. The size ratchet -- flash and RAM against bench/parity/ratchet.toml.
  #    Deterministic, so it gates at 0%.
  # 2. Counter parity -- insns/allocs/gcs must be EQUAL between the sim and
  #    hil rows of the same commit+app. Inequality is a runtime divergence
  #    (memory, threading, dispatch), not a performance signal
  #    (docs/parity-audit.md P1).
  #
  # What this deliberately no longer does is alarm on hil/sim wall-clock ratio
  # drift. That check was written at 30%, which is ~10,000x the 32 ppm device
  # floor and never fired once in four months. Worse, the ratio is not a
  # meaningful quantity: the sim is a biased predictor of device time, not
  # merely a noisy one (docs/perf-campaign-2026-08.md S4 measured it
  # over-predicting by seven points), so the ratio moves with every commit
  # that happens to suit one environment more than the other. Trending
  # `bench-report.py --trend wall_ms --env hil` is the check that works.
  python3 "$SCRIPT_DIR/bench-report.py" --ratchet || CHECK_BAD=1
  echo
  python3 - "$CSV" <<'EOF'
import collections, csv, sys

COUNTERS = ("insns", "allocs", "gcs", "bands", "fbytes")
rows = list(csv.DictReader(open(sys.argv[1])))
seen = collections.defaultdict(dict)
for r in rows:
    if r["metric"] in COUNTERS and r["env"] in ("sim", "hil"):
        seen[(r["commit"], r["app"], r["mode"], r["metric"])][r["env"]] = r["value"]

paired = [(k, v) for k, v in seen.items() if "sim" in v and "hil" in v]
if not paired:
    print("counter parity: no commit has both a sim and a hil row with "
          "parity-metrics enabled -- run ./scripts/parity-bench.sh --both")
    sys.exit(0)

bad = 0
for (commit, app, mode, metric), v in sorted(paired):
    if v["sim"] != v["hil"]:
        bad += 1
        print(f"counter parity: DIVERGENCE {commit} {app}[{mode}] {metric} "
              f"sim={v['sim']} hil={v['hil']}")
print(f"counter parity: {len(paired)} paired counter(s), {bad} divergence(s)")
sys.exit(1 if bad else 0)
EOF
  # An `[[ ]] && exit` here would make the script exit 1 whenever the
  # condition is false, because it is the last statement.
  if [[ "${CHECK_BAD:-0}" == "1" ]]; then
    exit 1
  fi
fi
