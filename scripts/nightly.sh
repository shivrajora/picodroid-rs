#!/usr/bin/env bash
# The bench machine's nightly: fuzz, then the sim matrix, then the HIL fleet,
# each stage starting only once the previous one has exited, so no two of them
# ever share the CPU, the net-test ports (7000/8000/8443) or the boards.
#
#   ./scripts/nightly.sh                 # the 1 AM cron: pull, then all three stages
#   ./scripts/nightly.sh --skip fuzz     # leave a stage out (repeatable: fuzz|sim|hil)
#   ./scripts/nightly.sh --no-pull       # run the tree as it is
#
# Pulls once and hands every stage --no-pull, so the whole night tests one
# commit. Each stage appends to its own build/<stage>/cron.log and sends its
# own email, as when they were separate cron entries; this script logs one line
# per stage (start, exit, duration). A failing stage does not stop the later
# ones; the exit status is non-zero when any stage failed. The per-stage
# timeouts are backstops against a hang, about twice each stage's usual length
# (fuzz 45 min, sim ~1 h 35 m, HIL ~3 h 45 m). If the previous night is still
# running, this one logs that and exits 0.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

# Where the stage scripts live; a test points this at stubs.
STAGE_DIR="${NIGHTLY_SCRIPTS_DIR:-$SCRIPT_DIR}"
FUZZ_TIMEOUT="${NIGHTLY_FUZZ_TIMEOUT:-3500}"
SIM_TIMEOUT="${NIGHTLY_SIM_TIMEOUT:-4h}"
HIL_TIMEOUT="${NIGHTLY_HIL_TIMEOUT:-7h}"

PULL=true
SKIP=" "
ARGS=("$@")
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-pull) PULL=false; shift ;;
    --skip)
      case "${2:-}" in
        fuzz|sim|hil) SKIP+="$2 "; shift 2 ;;
        *) echo "Unknown --skip value: ${2:-} (want fuzz|sim|hil)" >&2; exit 1 ;;
      esac
      ;;
    -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
    *) echo "Unknown option: $1 (see --help)" >&2; exit 1 ;;
  esac
done

log() { timestamp_log "$@"; }

mkdir -p "$REPO_ROOT/build"
exec 9> "$REPO_ROOT/build/nightly.lock"
if ! flock -n 9; then
  log "previous nightly still running (build/nightly.lock), skipping"
  exit 0
fi

# Pull, then re-exec so the pulled copy of this script runs the night. The
# lock fd is inherited across the exec, so the lock stays held.
if [[ "$PULL" == "true" && -z "${PICODROID_NIGHTLY_PULLED:-}" ]]; then
  log "Pulling latest code..."
  git -C "$REPO_ROOT" pull --ff-only 2>&1 | while IFS= read -r line; do log "  git: $line"; done || true
  PICODROID_NIGHTLY_PULLED=1 exec "$SCRIPT_DIR/nightly.sh" "${ARGS[@]}"
fi

log "========================================="
log "Nightly at $(git -C "$REPO_ROOT" rev-parse --short HEAD)"
log "========================================="

FAILED=()
# Args: name timeout log_dir command...
run_stage() {
  local name="$1" limit="$2" dir="$REPO_ROOT/build/$3"; shift 3
  if [[ "$SKIP" == *" $name "* ]]; then
    log "$name: SKIP (--skip)"
    return
  fi
  if [[ ! -x "$1" ]]; then
    log "$name: SKIP ($1 not found)"
    return
  fi
  mkdir -p "$dir"
  log "$name: start (timeout $limit, log build/${dir#"$REPO_ROOT/build/"}/cron.log)"
  local start=$SECONDS rc=0
  # 9>&-: a daemon a stage leaves behind must not hold the lock into tomorrow.
  (cd "$REPO_ROOT" && timeout -k 300 "$limit" "$@") >> "$dir/cron.log" 2>&1 9>&- || rc=$?
  local took=$((SECONDS - start))
  local note=""
  [[ $rc -eq 124 || $rc -eq 137 ]] && note=" (timed out)"
  log "$name: exit $rc$note after $((took / 3600))h$(((took % 3600) / 60))m$((took % 60))s"
  [[ $rc -eq 0 ]] || FAILED+=("$name")
}

run_stage fuzz "$FUZZ_TIMEOUT" fuzz "$STAGE_DIR/fuzz.sh" --nightly --no-pull
run_stage sim "$SIM_TIMEOUT" sim "$STAGE_DIR/sim-run.sh" --no-pull
run_stage hil "$HIL_TIMEOUT" hil "$STAGE_DIR/hil-fleet.sh" --no-pull

if [[ ${#FAILED[@]} -gt 0 ]]; then
  log "Nightly done; failed: ${FAILED[*]}"
  exit 1
fi
log "Nightly done; every stage passed"
