#!/usr/bin/env bash
# Nightly HIL over the whole bench fleet: one hil-run.sh per slot, all at
# the same time.
#
#   ./scripts/hil-fleet.sh                       # every slot in the fleet config
#   ./scripts/hil-fleet.sh --slots testbench_rp2350,pico_enviro_mon_w
#   ./scripts/hil-fleet.sh --app helloworld --no-email
#
# Done once, for everyone: git pull, the pdb and papk-pack host tools, and
# the network test servers (ports 7000/8000, needed by the net rows). Then
# each slot gets a runner of its own with a private cargo target directory
# and PAPK directory under build/hil/<slot>/ (two runners must never share a
# firmware ELF or a package file), results in build/hil/results/<slot>/,
# logs in build/hil/logs/<slot>/<run id>/ (console.log is the runner's own
# output) and its own email. The exit status is non-zero when any runner's
# is. Options other than --slots and --no-pull are passed to every runner:
# --app, --mode, --include-hw, --skip-pdb, --no-email.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

SLOTS=""
PULL=true
RUNNER_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --slots) SLOTS="$2"; shift 2 ;;
    --slots=*) SLOTS="${1#--slots=}"; shift ;;
    --no-pull) PULL=false; shift ;;
    --app|--mode) RUNNER_ARGS+=("$1" "$2"); shift 2 ;;
    --include-hw|--skip-pdb|--no-email) RUNNER_ARGS+=("$1"); shift ;;
    -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
    *) echo "Unknown option: $1 (see --help)" >&2; exit 1 ;;
  esac
done

log() { timestamp_log "$@"; }

fleet_enabled || { echo "ERROR: no fleet config at ${PICODROID_FLEET_CONF-$FLEET_CONF_DEFAULT} (see scripts/fleet.conf.example)" >&2; exit 1; }
fleet_check_conf || exit 1
[[ -n "$SLOTS" ]] || SLOTS="$(fleet_slots | paste -sd,)"
slots=()
for s in ${SLOTS//,/ }; do
  fleet_slot_row "$s" >/dev/null || { echo "ERROR: unknown slot '$s'" >&2; fleet_list_slots >&2; exit 1; }
  slots+=("$s")
done
[[ ${#slots[@]} -gt 0 ]] || { echo "ERROR: no slots selected" >&2; exit 1; }

# Pull once; the runners get --no-pull.
if [[ "$PULL" == "true" ]]; then
  log "Pulling latest code..."
  git -C "$REPO_ROOT" pull --ff-only 2>&1 | while IFS= read -r line; do log "  git: $line"; done || true
fi

COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
RUN_ID="$(date '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT_SHA}"
FLEET_LOG_DIR="$REPO_ROOT/build/hil/logs/fleet/$RUN_ID"
mkdir -p "$FLEET_LOG_DIR"

log "========================================="
log "HIL fleet run: $RUN_ID  slots: ${slots[*]}"
log "========================================="

# Host tools, built once into the shared target directory and handed to the
# runners; a runner builds its own copy only if this fails.
HOST_TARGET="$(host_target)"
log "Building pdb and papk-pack..."
if cargo build --release --quiet --target "$HOST_TARGET" \
     --manifest-path "$REPO_ROOT/tools/pdb/Cargo.toml" > "$FLEET_LOG_DIR/pdb-build.log" 2>&1 \
   && cargo build --release --quiet --target "$HOST_TARGET" \
     --manifest-path "$REPO_ROOT/tools/papk-pack/Cargo.toml" > "$FLEET_LOG_DIR/papk-pack-build.log" 2>&1; then
  export HIL_PDB_BIN="$REPO_ROOT/target/$HOST_TARGET/release/pdb"
  export HIL_PAPK_PACK_BIN="$REPO_ROOT/target/$HOST_TARGET/release/papk-pack"
else
  log "WARNING: host tool build failed (see $FLEET_LOG_DIR/*-build.log); runners build their own"
fi

# Network test servers: once, for every runner, when a selected slot has a
# network board and the credentials exist. The ports are host-global.
board_has_network() {
  local toml
  toml=$(find "$REPO_ROOT/platforms" -path "*/boards/$1/board.toml" | head -1)
  [[ -n "$toml" ]] && grep -qE '^has_network[[:space:]]*=[[:space:]]*true' "$toml"
}
want_net=false
for s in "${slots[@]}"; do
  for b in $(fleet_slot_boards "$s"); do
    if board_has_network "$b"; then want_net=true; fi
  done
done
trap 'stop_net_listeners' EXIT
if [[ "$want_net" == "true" && -f "$REPO_ROOT/.wifi-creds.env" ]]; then
  if start_net_listeners "$FLEET_LOG_DIR"; then
    export PICODROID_NET_LISTENERS_EXTERNAL=1
    log "Net listeners up (echo $NET_ECHO_PORT, http $NET_HTTP_PORT), shared by every runner"
  else
    log "WARNING: $NET_LISTENER_ERR -- net rows will report it"
  fi
fi

# Share the CPU between the runners' firmware builds.
jobs=$(( $(cpu_count) / ${#slots[@]} ))
(( jobs >= 2 )) || jobs=2

declare -A pids=()
for s in "${slots[@]}"; do
  work="$REPO_ROOT/build/hil/$s"
  slot_log_dir="$REPO_ROOT/build/hil/logs/$s/$RUN_ID"
  mkdir -p "$work/target" "$work/apks" "$slot_log_dir"
  log "Starting runner for slot $s (board $(fleet_slot_primary "$s"), console $slot_log_dir/console.log)"
  env CARGO_TARGET_DIR="$work/target" HIL_APK_DIR="$work/apks" HIL_RUN_ID="$RUN_ID" HIL_JOBS="$jobs" \
    bash "$SCRIPT_DIR/hil-run.sh" --slot "$s" --no-pull ${RUNNER_ARGS[@]+"${RUNNER_ARGS[@]}"} \
    > "$slot_log_dir/console.log" 2>&1 &
  pids[$s]=$!
done

# A signal stops every runner (each releases its own lease on exit).
trap 'for p in "${pids[@]}"; do kill -TERM "$p" 2>/dev/null || true; done; stop_net_listeners' INT TERM

fleet_rc=0
for s in "${slots[@]}"; do
  rc=0
  wait "${pids[$s]}" || rc=$?
  results="$REPO_ROOT/build/hil/results/$s/$RUN_ID.txt"
  if [[ -f "$results" ]]; then
    summary="$(sort "$results" | cut -d' ' -f1 | uniq -c | awk '{printf "%s %s ", $2, $1}')"
  else
    summary="no results file"
  fi
  log "Slot $s: exit $rc  $summary"
  [[ $rc -eq 0 ]] || fleet_rc=1
done

log "HIL fleet run $RUN_ID complete (exit $fleet_rc)"
exit "$fleet_rc"
