#!/usr/bin/env bash
# Measured Rust coverage (docs/coverage.md), in two layers:
#
#   unit  the host unit tests, exactly as scripts/test.sh runs them (the
#         workspace, both shrink modes), instrumented by cargo-llvm-cov
#   sim   the hil-tests.conf rows and sim-run lanes, on an instrumented sim
#         (scripts/sim-run.sh --coverage): the FreeRTOS-POSIX backing, boot,
#         the lifecycle and the native handlers no unit test reaches
#
# Usage:
#   ./scripts/coverage.sh                    # unit layer (what CI runs)
#   ./scripts/coverage.sh --sim              # unit + every sim row, merged
#   ./scripts/coverage.sh --sim --apps qa_lang,qa_life
#   ./scripts/coverage.sh --check            # unit layer against the baseline
#   ./scripts/coverage.sh --accept           # rewrite the baseline
#
# Output (default build/coverage/): summary.md (per-crate table), summary.json,
# lcov.info, html/index.html and uncompiled.txt, for the unit layer and, with
# --sim, again under sim/ for the merged unit + sim profile. Instrumented
# builds live in target/llvm-cov-target, so target/ is never rebuilt for them.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

SIM=false
APPS=""
CHECK=false
ACCEPT=false
OUT="$REPO_ROOT/build/coverage"
HISTORY=""
BASELINE="$SCRIPT_DIR/coverage-baseline.conf"
# Percentage points a crate may move before --check calls it a change. Set
# from the spread of repeated runs of the same tree (docs/coverage.md).
TOLERANCE="${PICODROID_COVERAGE_TOLERANCE:-0.5}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --unit) shift ;;
    --sim) SIM=true; shift ;;
    --apps) SIM=true; APPS="$2"; shift 2 ;;
    --check) CHECK=true; shift ;;
    --accept) ACCEPT=true; shift ;;
    --out) OUT="$2"; shift 2 ;;
    --history) HISTORY="$2"; shift 2 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
done
[[ "$OUT" == /* ]] || OUT="$PWD/$OUT"
[[ -z "$HISTORY" || "$HISTORY" == /* ]] || HISTORY="$PWD/$HISTORY"

if ! cargo llvm-cov --version > /dev/null 2>&1; then
  echo "ERROR: cargo-llvm-cov is not installed. Install it and the LLVM tools with:" >&2
  echo "  cargo install cargo-llvm-cov --locked" >&2
  echo "  rustup component add llvm-tools-preview" >&2
  exit 1
fi

HOST_TARGET="$(host_target)"
COMMIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
# cargo-llvm-cov's own directory under the cargo target dir; the sim layer
# must build and leave its profiles in the same place for one report.
COV_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}/llvm-cov-target"

# Test code and vendored code are not in the denominator. Inline
# #[cfg(test)] modules cannot be told apart on stable Rust; they count.
# coverage-summary.py walks the tree with the same filter.
IGNORE_RE='(/tests?/|/tests?\.rs$|_tests?\.rs$|/third_party/|/benches/|/examples/)'

# report DIR TITLE: the reports of whatever profiles are in COV_TARGET_DIR now.
report() {
  local dir="$1" title="$2"
  mkdir -p "$dir"
  rm -rf "$dir/html"
  # An instrumented process that died before its runtime could write anything
  # leaves an empty file, which llvm-profdata rejects.
  find "$COV_TARGET_DIR" -maxdepth 1 -name '*.profraw' -size 0 -delete
  local -a args=(--target "$HOST_TARGET" --ignore-filename-regex "$IGNORE_RE")
  cargo llvm-cov report "${args[@]}" --json --summary-only --output-path "$dir/summary.json"
  cargo llvm-cov report "${args[@]}" --lcov --output-path "$dir/lcov.info"
  cargo llvm-cov report "${args[@]}" --html --output-dir "$dir"
  python3 "$SCRIPT_DIR/coverage-summary.py" "$dir/summary.json" \
    --markdown "$dir/summary.md" --title "$title ($COMMIT_SHA)" \
    --uncompiled "$dir/uncompiled.txt"
}

echo "==> Cleaning previous coverage profiles and instrumented workspace builds..."
cargo llvm-cov clean --workspace

echo "==> Unit layer (scripts/test.sh --coverage)..."
bash "$SCRIPT_DIR/test.sh" --coverage
report "$OUT" "Rust unit-test coverage"
[[ -n "$HISTORY" ]] && python3 "$SCRIPT_DIR/coverage-summary.py" "$OUT/summary.json" \
  --history "$HISTORY" --layer unit --commit "$COMMIT_SHA"

sim_rc=0
if [[ "$SIM" == true ]]; then
  echo "==> Sim layer (scripts/sim-run.sh --coverage)..."
  sim_args=(--coverage --no-email --no-pull)
  if [[ -n "$APPS" ]]; then
    IFS=',' read -r -a app_list <<< "$APPS"
    for app in "${app_list[@]}"; do
      CARGO_TARGET_DIR="$COV_TARGET_DIR" bash "$SCRIPT_DIR/sim-run.sh" "${sim_args[@]}" --app "$app" || sim_rc=1
    done
  else
    CARGO_TARGET_DIR="$COV_TARGET_DIR" bash "$SCRIPT_DIR/sim-run.sh" "${sim_args[@]}" || sim_rc=1
  fi
  report "$OUT/sim" "Rust coverage, unit tests + sim rows"
  [[ -n "$HISTORY" ]] && python3 "$SCRIPT_DIR/coverage-summary.py" "$OUT/sim/summary.json" \
    --history "$HISTORY" --layer unit+sim --commit "$COMMIT_SHA"
fi

echo
cat "$OUT/summary.md"
[[ "$SIM" == true ]] && cat "$OUT/sim/summary.md"
echo "HTML: $OUT/html/index.html"
[[ "$SIM" == true ]] && echo "HTML (unit + sim): $OUT/sim/html/index.html"

rc=0
if [[ "$ACCEPT" == true ]]; then
  python3 "$SCRIPT_DIR/coverage-summary.py" "$OUT/summary.json" --write-baseline "$BASELINE"
  echo "Commit it with a 'coverage:' trailer naming the crates that moved and why."
elif [[ "$CHECK" == true ]]; then
  python3 "$SCRIPT_DIR/coverage-summary.py" "$OUT/summary.json" \
    --check "$BASELINE" --tolerance "$TOLERANCE" || rc=1
fi

if [[ $sim_rc -ne 0 ]]; then
  echo "NOTE: some sim rows failed (build/sim/results/); their coverage is still in the report." >&2
  rc=1
fi
exit $rc
