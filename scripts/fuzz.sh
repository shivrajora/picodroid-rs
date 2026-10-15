#!/usr/bin/env bash
# Fuzz the parsers that take untrusted bytes (docs/designs/fuzzing-2026-10.md).
#
# The targets are plain functions in crates/fuzz-targets; fuzz/ is the
# cargo-fuzz crate that wraps them for libFuzzer (nightly). fuzz/regress/<t>/
# holds each target's committed seeds and the minimized reproducer of every
# past finding; the stable replay of it runs in scripts/test.sh.
#
# Usage:
#   ./scripts/fuzz.sh --list                         # the targets
#   ./scripts/fuzz.sh --replay                       # fuzz/regress through every target (stable)
#   ./scripts/fuzz.sh --target papk --time 300       # fuzz one target for 5 minutes
#   ./scripts/fuzz.sh --target papk,install          # several, --time each (default 60 s)
#   ./scripts/fuzz.sh --target all --time 60         # every target in turn
#   ./scripts/fuzz.sh --target papk --reproduce FILE # run one input (a crash) under the fuzz build
#   ./scripts/fuzz.sh --build                        # build every target (the --full pre-commit stage)
#   ./scripts/fuzz.sh --nightly                      # the 1 AM cron: pull, every target, mail
#
# Options:
#   --jobs N        libFuzzer worker processes per target (default: cores - 2)
#   --budget MIN    --nightly's total fuzzing minutes, split by weight (default 45:
#                   the 1 AM run must end before the 2 AM sim nightly)
#   --no-email      --nightly without the mail
#   --no-pull       --nightly on the tree as it is
#   --weekly        also run the Sunday chores: corpus minimization and the
#                   class-link replay under Miri (--nightly does it on Sundays)
#
# The living corpus is build/fuzz/corpus/<t>/ (untracked, kept across runs);
# crashes land in build/fuzz/crashes/<run id>/<t>/, minimized; logs in
# build/fuzz/logs/<run id>/ and one PASS/FAIL line per target in
# build/fuzz/results/<run id>.txt, the format sim-run and hil-run write.
#
# Needs: rustup's nightly toolchain and `cargo install cargo-fuzz --locked`.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

FUZZ_DIR="$REPO_ROOT/fuzz"
OUT="$REPO_ROOT/build/fuzz"

# name|weight|sanitizer[|noleaks]. Weight: share of the
# nightly budget. ASan where the target reaches `unsafe` (class-link's
# in-place tables, the installer's flash mapping, the TLS session's buffers,
# the interpreter's heaps); none for the crates that forbid it, where it
# buys nothing and halves the rate. `noleaks`: the target leaks on purpose
# (`interp`: `load_class` wants `'static` bytes, as flash is), so leak
# detection is off, per input and at exit.
TARGETS=(
  "papk|3|address"
  "papk_prefix|1|none"
  "papk_roundtrip|2|address"
  "class_section|3|address"
  "link_class|3|address"
  "bytecode_check|1|address"
  "interp|3|address|noleaks"
  "pdb_decode|1|none"
  "install|2|address"
  "http_head|1|none"
  "http_chunked|1|none"
  "json|2|none"
  "der|1|none"
  "tls_server|3|address"
  "records|1|none"
)

MODE=""
SELECTED=""
TIME=60
REPRODUCE=""
JOBS=""
BUDGET=45
EMAIL=true
PULL=true
WEEKLY=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --list) MODE=list; shift ;;
    --replay) MODE=replay; shift ;;
    --build) MODE=build; shift ;;
    --nightly) MODE=nightly; shift ;;
    --target) SELECTED="$2"; [[ -n "$MODE" ]] || MODE=run; shift 2 ;;
    --time) TIME="$2"; shift 2 ;;
    --reproduce) REPRODUCE="$2"; MODE=reproduce; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --budget) BUDGET="$2"; shift 2 ;;
    --no-email) EMAIL=false; shift ;;
    --no-pull) PULL=false; shift ;;
    --weekly) WEEKLY=true; shift ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
done
[[ -n "$MODE" ]] || { sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 2; }
[[ -n "$JOBS" ]] || JOBS=$(( $(cpu_count) > 3 ? $(cpu_count) - 2 : 1 ))

field() { local IFS="|"; read -ra f <<< "$1"; echo "${f[$2]:-}"; }
target_row() {
  local row
  for row in "${TARGETS[@]}"; do
    [[ "$(field "$row" 0)" == "$1" ]] && { echo "$row"; return 0; }
  done
  echo "ERROR: no fuzz target '$1' (--list)" >&2
  return 1
}

need_cargo_fuzz() {
  if ! cargo +nightly fuzz --version > /dev/null 2>&1; then
    echo "ERROR: cargo-fuzz on the nightly toolchain is needed. Install it with:" >&2
    echo "  rustup toolchain install nightly && cargo install cargo-fuzz --locked" >&2
    exit 1
  fi
}

# fuzz/ is its own workspace, so it cannot inherit the root's
# [patch.crates-io]; it restates the entries its graph reaches. Refuse to
# fuzz a crate graph the device does not run: every entry must match the
# root's, and no crate the root patches may come from the registry.
check_patches() {
  python3 - "$REPO_ROOT/Cargo.toml" "$FUZZ_DIR/Cargo.toml" "$FUZZ_DIR/Cargo.lock" << 'EOF'
import re, sys, pathlib
def patches(path, prefix=""):
    text = pathlib.Path(path).read_text()
    m = re.search(r"^\[patch\.crates-io\]\n(.*?)(?=^\[|\Z)", text, re.S | re.M)
    out = {}
    for name, p in re.findall(r'^([\w-]+)\s*=\s*\{\s*path\s*=\s*"([^"]+)"', m.group(1) if m else "", re.M):
        out[name] = p[len(prefix):] if p.startswith(prefix) else "?" + p
    return out
root, fuzz = patches(sys.argv[1]), patches(sys.argv[2], "../")
bad = [f"{n}: fuzz/ has {p}, the root {root.get(n)}" for n, p in fuzz.items() if root.get(n) != p]
lock = pathlib.Path(sys.argv[3])
if lock.exists():
    for block in lock.read_text().split("[[package]]"):
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        if name and name.group(1) in root and "source = \"registry" in block:
            bad.append(f"{name.group(1)} comes from the registry in fuzz/; add the root's patch")
if bad:
    print("ERROR: fuzz/Cargo.toml [patch.crates-io] drifted from the root's:", *bad, sep="\n  ")
    sys.exit(1)
EOF
}

# The root's lockfile as the starting point, so the fuzz build resolves the
# versions the firmware does (Cargo.lock is untracked; see
# reference_worktree_cargo_config_merge in the bench notes).
seed_lockfile() {
  [[ -f "$FUZZ_DIR/Cargo.lock" || ! -f "$REPO_ROOT/Cargo.lock" ]] || cp "$REPO_ROOT/Cargo.lock" "$FUZZ_DIR/Cargo.lock"
}

fuzz_cmd() { (cd "$REPO_ROOT" && cargo +nightly fuzz "$@"); }

build_target() {
  local name="$1" san
  san="$(field "$(target_row "$name")" 2)"
  fuzz_cmd build -s "$san" -a "$name"
}

# Fuzz one target for $2 seconds. Returns 0 clean, 1 on a finding (the input
# minimized into $3), 2 when it did not build or run.
run_target() {
  local name="$1" secs="$2" crash_dir="$3" log="$4" san corpus dict=()
  san="$(field "$(target_row "$name")" 2)"
  corpus="$OUT/corpus/$name"
  mkdir -p "$corpus" "$crash_dir"
  [[ -f "$FUZZ_DIR/dict/$name.dict" ]] && dict=("-dict=$FUZZ_DIR/dict/$name.dict")
  local asan="${ASAN_OPTIONS:-}"
  if [[ "$(field "$(target_row "$name")" 3)" == noleaks ]]; then
    dict+=("-detect_leaks=0")
    asan="${asan:+$asan:}detect_leaks=0"
  fi
  # -fork: workers in parallel, restarted on a crash; the first finding ends
  # the run. -timeout: an input slower than 10 s is a hang worth reporting.
  local rc=0
  ASAN_OPTIONS="$asan" fuzz_cmd run -s "$san" -a "$name" "$corpus" "$FUZZ_DIR/regress/$name" -- \
    -max_total_time="$secs" -fork="$JOBS" -timeout=10 -rss_limit_mb=2048 \
    -artifact_prefix="$crash_dir/" -print_final_stats=1 "${dict[@]}" \
    > "$log" 2>&1 || rc=$?
  local found
  found="$(find "$crash_dir" -maxdepth 1 -type f \( -name 'crash-*' -o -name 'timeout-*' -o -name 'oom-*' -o -name 'leak-*' \) | head -1)"
  if [[ -n "$found" ]]; then
    {
      echo
      echo "==> minimizing $(basename "$found")"
      # Bounded: minimizing a hang reruns it, ten seconds a try.
      ASAN_OPTIONS="$asan" fuzz_cmd tmin -s "$san" -a "$name" "$found" -r 2000 -- \
        -max_total_time=120 -timeout=10 2>&1 | tail -20 || true
      echo "==> reproduce: ./scripts/fuzz.sh --target $name --reproduce $found"
    } >> "$log"
    return 1
  fi
  [[ $rc -eq 0 ]] && return 0
  return 2
}

case "$MODE" in
  list)
    for row in "${TARGETS[@]}"; do
      printf '%-16s weight %s  sanitizer %s\n' "$(field "$row" 0)" "$(field "$row" 1)" "$(field "$row" 2)"
    done
    exit 0
    ;;
  replay)
    cd "$REPO_ROOT"
    exec cargo test -p fuzz-targets --test replay
    ;;
  build)
    need_cargo_fuzz
    seed_lockfile
    for row in "${TARGETS[@]}"; do
      build_target "$(field "$row" 0)"
    done
    check_patches
    exit 0
    ;;
  reproduce)
    need_cargo_fuzz
    [[ -n "$SELECTED" ]] || { echo "ERROR: --reproduce needs --target" >&2; exit 2; }
    san="$(field "$(target_row "$SELECTED")" 2)"
    [[ "$REPRODUCE" == /* ]] || REPRODUCE="$PWD/$REPRODUCE"
    fuzz_cmd run -s "$san" -a "$SELECTED" "$REPRODUCE"
    exit $?
    ;;
  run)
    need_cargo_fuzz
    seed_lockfile
    if [[ "$SELECTED" == all ]]; then
      names=()
      for row in "${TARGETS[@]}"; do names+=("$(field "$row" 0)"); done
    else
      IFS=',' read -ra names <<< "$SELECTED"
    fi
    for name in "${names[@]}"; do target_row "$name" > /dev/null; done
    run_id="$(date '+%Y-%m-%d_%Hh%Mm%Ss')_manual"
    status=0
    for name in "${names[@]}"; do
      build_target "$name"
      check_patches
      log="$OUT/logs/$run_id/fuzz-$name.log"
      mkdir -p "$(dirname "$log")"
      timestamp_log "fuzz $name: ${TIME}s on $JOBS workers (log $log)"
      if run_target "$name" "$TIME" "$OUT/crashes/$run_id/$name" "$log"; then
        grep -E '^(stat::|Done|#[0-9]+.*DONE)' "$log" | tail -4 || true
        timestamp_log "fuzz $name: clean"
      else
        tail -40 "$log"
        timestamp_log "fuzz $name: FINDING or error (log $log)"
        status=1
      fi
    done
    exit $status
    ;;
esac

# ── --nightly ───────────────────────────────────────────────────────────────

need_cargo_fuzz
if [[ "$PULL" == true ]]; then
  timestamp_log "Pulling latest code..."
  git -C "$REPO_ROOT" pull --ff-only 2>&1 | while IFS= read -r line; do timestamp_log "  git: $line"; done || true
fi
COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
RUN_ID="$(date '+%Y-%m-%d_%Hh%Mm%Ss')_${COMMIT_SHA}"
LOG_DIR="$OUT/logs"
RUN_LOG_DIR="$LOG_DIR/$RUN_ID"
RESULTS_DIR="$OUT/results"
RESULTS_FILE="$RESULTS_DIR/$RUN_ID.txt"
mkdir -p "$RUN_LOG_DIR" "$RESULTS_DIR"
: > "$RESULTS_FILE"
timestamp_log "Fuzz run $RUN_ID: ${BUDGET} min over ${#TARGETS[@]} targets, $JOBS workers"

seed_lockfile
# The stable replay first: a regression of a past finding is the loudest
# result a night can have, and needs no fuzzing to show.
if (cd "$REPO_ROOT" && cargo test -p fuzz-targets --test replay) > "$RUN_LOG_DIR/fuzz-replay.log" 2>&1; then
  echo "PASS fuzz-replay" >> "$RESULTS_FILE"
else
  echo "FAIL fuzz-replay" >> "$RESULTS_FILE"
fi
if ! check_patches > "$RUN_LOG_DIR/fuzz-patches.log" 2>&1; then
  echo "FAIL fuzz-patches" >> "$RESULTS_FILE"
fi

# Sunday's chores below take their time out of the fuzzing, so the run
# still ends before the 2 AM sim nightly wants the machine.
SUNDAY=false
[[ "$WEEKLY" == true || "$(date +%u)" == 7 ]] && SUNDAY=true
[[ "$SUNDAY" == true ]] && BUDGET=$(( BUDGET > 25 ? BUDGET - 15 : BUDGET ))

total_weight=0
for row in "${TARGETS[@]}"; do total_weight=$(( total_weight + $(field "$row" 1) )); done
for row in "${TARGETS[@]}"; do
  name="$(field "$row" 0)"
  secs=$(( BUDGET * 60 * $(field "$row" 1) / total_weight ))
  log="$RUN_LOG_DIR/fuzz-$name.log"
  timestamp_log "fuzz $name: ${secs}s"
  if ! build_target "$name" > "$log" 2>&1; then
    echo "ERROR fuzz-$name" >> "$RESULTS_FILE"
    continue
  fi
  rc=0
  run_target "$name" "$secs" "$OUT/crashes/$RUN_ID/$name" "$log.run" || rc=$?
  cat "$log.run" >> "$log" && rm -f "$log.run"
  case $rc in
    0) echo "PASS fuzz-$name" >> "$RESULTS_FILE" ;;
    1) echo "FAIL fuzz-$name" >> "$RESULTS_FILE" ;;
    *) echo "ERROR fuzz-$name" >> "$RESULTS_FILE" ;;
  esac
done

if [[ "$SUNDAY" == true ]]; then
  # Keep each corpus to the inputs that still add coverage.
  for row in "${TARGETS[@]}"; do
    name="$(field "$row" 0)"
    corpus="$OUT/corpus/$name"
    [[ -d "$corpus" ]] || continue
    timestamp_log "cmin $name ($(find "$corpus" -type f | wc -l) inputs)"
    ASAN_OPTIONS=detect_leaks=0 fuzz_cmd cmin -s "$(field "$row" 2)" -a "$name" "$corpus" >> "$RUN_LOG_DIR/fuzz-cmin.log" 2>&1 || true
  done
  # The in-place reads of class-link and the JVM heaps under Miri, over the
  # committed inputs (the replay reads files, so isolation is off; the
  # targets leak on purpose, as `load_class` and flash do). Tree
  # Borrows: under Stacked Borrows `Link::from_raw` reaches the pool through
  # a pointer taken from the table slice (docs/designs/fuzzing-2026-10.md §8).
  if (cd "$REPO_ROOT" && PICODROID_FUZZ_REPLAY=class_section,link_class,bytecode_check,papk,interp \
      MIRIFLAGS="${MIRIFLAGS:-} -Zmiri-disable-isolation -Zmiri-tree-borrows -Zmiri-ignore-leaks" \
      cargo +nightly miri test -p fuzz-targets --test replay) > "$RUN_LOG_DIR/fuzz-miri.log" 2>&1; then
    echo "PASS fuzz-miri" >> "$RESULTS_FILE"
  else
    echo "FAIL fuzz-miri" >> "$RESULTS_FILE"
  fi
fi

timestamp_log "Results ($RESULTS_FILE):"
sed 's/^/  /' "$RESULTS_FILE"
if [[ "$EMAIL" == true ]]; then
  python3 "$SCRIPT_DIR/hil-email.py" --results "$RESULTS_FILE" --log-dir "$LOG_DIR" \
    --run-id "$RUN_ID" --sha "$COMMIT_SHA" --suite fuzz 2>&1 |
    while IFS= read -r line; do timestamp_log "  email: $line"; done ||
    timestamp_log "  Email sending failed (non-fatal)."
fi
! grep -qE '^(FAIL|ERROR) ' "$RESULTS_FILE"
