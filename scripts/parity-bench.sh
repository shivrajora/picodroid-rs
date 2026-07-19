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
#   - wall_ms is meaningful only as a hil/sim RATIO tracked over time
#     (`--check` alarms when it drifts >30% from its trailing median). A
#     host wall-clock number never predicts device wall-clock.
#
# Usage:
#   ./scripts/parity-bench.sh                  # sim lane, all bench apps
#   ./scripts/parity-bench.sh --app benchmark  # one app
#   ./scripts/parity-bench.sh --hil            # HIL lane (board + probe attached)
#   ./scripts/parity-bench.sh --check          # ratio-drift check over the CSV
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

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)   APPS=("$2"); shift 2 ;;
    --board) BOARD="$2"; shift 2 ;;
    --hil)   DO_HIL=true; DO_SIM=false; shift ;;
    --both)  DO_HIL=true; DO_SIM=true; shift ;;
    --check) DO_CHECK=true; DO_SIM=false; shift ;;
    -h|--help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

mkdir -p "$(dirname "$CSV")"
[[ -f "$CSV" ]] || echo "utc,commit,env,app,metric,value" > "$CSV"

UTC="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
COMMIT="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
HOST_TARGET="$(host_target)"

emit() { # env app metric value
  echo "$UTC,$COMMIT,$1,$2,$3,$4" >> "$CSV"
}

# Parse one captured log: TOTAL/SCORE wall numbers + the parity counter line.
# Tolerant of both sim ("[Benchmark] TOTAL...") and defmt ("Benchmark: TOTAL")
# tag framing.
parse_log() { # env app logfile
  local env_name="$1" app="$2" log="$3"
  local wall
  wall="$(grep -oE 'TOTAL: [0-9]+ ms' "$log" | tail -1 | grep -oE '[0-9]+' || true)"
  [[ -z "$wall" ]] && wall="$(grep -oE 'SCORE[:=] ?[0-9]+' "$log" | tail -1 | grep -oE '[0-9]+' || true)"
  [[ -n "$wall" ]] && emit "$env_name" "$app" wall_ms "$wall"
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
    local_log="$LOG_DIR/$app.sim.log"
    # Bench apps terminate on their own; the timeout is a hang backstop.
    PICODROID_APK_PATH="$REPO_ROOT/build/apks/$app.papk" \
      PICODROID_SIM_HEADLESS=1 \
      timeout 600 "$REPO_ROOT/target/$HOST_TARGET/release/picodroid" \
      > "$local_log" 2>&1 || true
    parse_log sim "$app" "$local_log"
    echo "  $app: $(grep -cE 'parity:' "$local_log") parity line(s) captured"
  done
fi

if $DO_HIL; then
  echo "==> HIL parity bench (board $BOARD; flashes the attached device)"
  LOG_DIR="${LOG_DIR:-$(mktemp -d)}"
  for app in "${APPS[@]}"; do
    local_log="$LOG_DIR/$app.hil.log"
    # flash.sh stays attached streaming RTT; bench apps print their totals
    # within the capture window, then we cut the session.
    # PICODROID_EXTRA_FEATURES is appended to the firmware feature set by
    # lib.sh::build_firmware.
    PICODROID_EXTRA_FEATURES=parity-metrics \
      timeout 300 "$SCRIPT_DIR/flash.sh" -b "$BOARD" -a "$app" -r \
      > "$local_log" 2>&1 || true
    parse_log hil "$app" "$local_log"
    echo "  $app: captured"
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
