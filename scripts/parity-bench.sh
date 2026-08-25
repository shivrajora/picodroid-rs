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
COMMIT="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
stamp_utc() { UTC="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"; }
HOST_TARGET="$(host_target)"

# Same-image batches are ~32 ppm reproducible. A spread above 0.5% means the
# board is in a bad state (thermal / USB / probe), not that the code changed --
# discard and re-run rather than averaging it in.
# (docs/perf-campaign-2026-08.md S1.3)
report_spread() { # app  < values-on-stdin
  awk -v app="$1" '
    /^[0-9]+$/ { v[n++] = $1; if (mn == "" || $1 < mn) mn = $1; if ($1 > mx) mx = $1 }
    END {
      if (n == 0) { printf "  %s: no wall-clock captured\n", app; exit }
      if (n == 1) { printf "  %s: 1 run, %d ms\n", app, mn; exit }
      p = (mx - mn) / mn * 100
      printf "  %s: %d runs, %d..%d ms, p2p %.3f%%%s\n", app, n, mn, mx, p,
             (p > 0.5 ? "  <== BAD BATCH, board unstable, re-run" : "")
    }'
}

emit() { # env app metric value
  local split=holdout
  is_train "$2" && split=train
  local mode=no-shrink
  [[ "${PICODROID_SHRINK:-0}" == "1" ]] && mode=shrink
  echo "$UTC,$COMMIT,$1,${EMIT_BOARD:-$BOARD},$2,$mode,$split,$3,$4" >> "$CSV"
}

# Parse one captured log: TOTAL/SCORE wall numbers + the parity counter line.
# Tolerant of both sim ("[Benchmark] TOTAL...") and defmt ("Benchmark: TOTAL")
# tag framing.
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
  for board in "${BOARDS[@]}"; do
    resolve_board "$board"
    EMIT_BOARD="$board"
    APP="${SIZE_APP:-helloworld}"
    PROFILE=release
    EXTRA_ARGS=(--release)
    build_firmware > /dev/null 2>&1 || { echo "  $board: BUILD FAILED"; continue; }
    # print_memory_usage exported TEXT/DATA/BSS; resolve_board exported the
    # ceilings. Headroom is emitted too: at 96-98% full it is the number that
    # decides whether a change can land at all.
    flash=$(( TEXT + DATA ))
    ram=$(( DATA + BSS ))
    stamp_utc
    emit size "$APP" text "$TEXT"
    emit size "$APP" data "$DATA"
    emit size "$APP" bss "$BSS"
    emit size "$APP" flash_bytes "$flash"
    emit size "$APP" ram_bytes "$ram"
    emit size "$APP" flash_headroom_bytes "$(( PROGRAM_FLASH_MAX - flash ))"
    emit size "$APP" ram_headroom_bytes "$(( RAM_MAX - ram ))"
    printf "  %-22s flash %8d / %8d (%2d%%, %6d free)   ram %7d / %7d (%2d%%, %6d free)\n" \
      "$board" "$flash" "$PROGRAM_FLASH_MAX" \
      "$(( flash * 100 / PROGRAM_FLASH_MAX ))" "$(( PROGRAM_FLASH_MAX - flash ))" \
      "$ram" "$RAM_MAX" "$(( ram * 100 / RAM_MAX ))" "$(( RAM_MAX - ram ))"
  done
  unset EMIT_BOARD
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
    for run in $(seq 1 "$n_runs"); do
      local_log="$LOG_DIR/$app.sim.$run.log"
      # Bench apps terminate on their own; the timeout is a hang backstop.
      PICODROID_APK_PATH="$REPO_ROOT/build/apks/$app.papk" \
        PICODROID_SIM_HEADLESS=1 \
        timeout 600 "$REPO_ROOT/target/$HOST_TARGET/release/picodroid" \
        > "$local_log" 2>&1 || true
      stamp_utc
      parse_log sim "$app" "$local_log"
      grep -oE 'TOTAL: [0-9]+ ms' "$local_log" | grep -oE '[0-9]+' \
        >> "$LOG_DIR/$app.sim.walls" || true
    done
    report_spread "$app" < "$LOG_DIR/$app.sim.walls"
  done
fi

if $DO_HIL; then
  echo "==> HIL parity bench (board $BOARD; flashes the attached device)"
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
    for run in $(seq 1 "$n_runs"); do
      local_log="$LOG_DIR/$app.hil.$run.log"
      PICODROID_EXTRA_FEATURES=parity-metrics \
        timeout 300 "$SCRIPT_DIR/flash.sh" -b "$BOARD" -a "$app" -r \
        > "$local_log" 2>&1 || true
      stamp_utc
      parse_log hil "$app" "$local_log"
      grep -oE 'TOTAL: [0-9]+ ms' "$local_log" | grep -oE '[0-9]+' \
        >> "$LOG_DIR/$app.hil.walls" || true
    done
    report_spread "$app" < "$LOG_DIR/$app.hil.walls"
  done
fi

if $DO_CHECK; then
  # For each app: latest hil/sim wall_ms ratio vs the median of prior
  # ratios. Exit 1 on >30% drift. Requires >= 3 prior paired runs.
  python3 - "$CSV" <<'EOF'
import csv, statistics, sys
rows = list(csv.DictReader(open(sys.argv[1])))
pairs = {}
for r in rows:
    if r["metric"] != "wall_ms":
        continue
    key = (r["commit"], r["app"])
    pairs.setdefault(key, {})[r["env"]] = float(r["value"])
ratios = {}
order = []
for (commit, app), envs in pairs.items():
    if "sim" in envs and "hil" in envs and envs["sim"] > 0:
        ratios.setdefault(app, []).append(envs["hil"] / envs["sim"])
        order.append(app)
bad = False
for app, rs in ratios.items():
    if len(rs) < 4:
        print(f"{app}: {len(rs)} paired run(s) — need 4+ for drift check")
        continue
    latest, prior = rs[-1], rs[:-1]
    med = statistics.median(prior)
    drift = abs(latest - med) / med
    status = "DRIFT" if drift > 0.30 else "ok"
    print(f"{app}: hil/sim ratio {latest:.2f} vs median {med:.2f} ({drift:+.0%}) {status}")
    bad |= drift > 0.30
sys.exit(1 if bad else 0)
EOF
fi
