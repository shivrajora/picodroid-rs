#!/usr/bin/env bash
# Source-tree invariants that are pure text checks: no compiler, seconds long.
# Shared by scripts/pre-commit (its `twins` and `cfg_gates` stages) and the CI
# `guards` job, so a bypassed hook (--no-verify, a merge) still meets them on
# the server.
#
#   ./scripts/check-source-guards.sh              # both checks
#   ./scripts/check-source-guards.sh --twins      # shadow twins only
#   ./scripts/check-source-guards.sh --cfg-gates  # cfg-gate hygiene only
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

check_twins() {
  # The extraction moved ~26k LOC out of platforms/rp/src. A move that leaves a
  # same-named file behind compiles fine and then silently diverges: commit
  # fc896b3 is the precedent, and ESP's removed scaffold accumulated 17 such
  # twins before anyone noticed. So: no relative path may exist under both
  # trees, except the ones below.
  #
  # The exceptions are two ends of one seam, not copies:
  #   gc_root_registration.rs — one provider list per crate is the design
  #                             (docs/designs/shared-core-extraction.md §3.G)
  #   hal/mod.rs              — core's is the trait/facade surface, the
  #                             family's is the rp-vs-sim routing
  #   pdb/mod.rs              — core's is the wire protocol, the family's wires
  #                             its four impls into it. Same shape as hal/mod.rs:
  #                             one name, two ends of a seam.
  #   fs/mod.rs               — core's is LittleFS itself (behind the `littlefs`
  #                             feature); the family's picks the backing store
  #                             and re-exports. Same shape again.
  #
  # hal/sim/mod.rs was a third exception until the simulator's last three stubs
  # moved to core; the family's copy is gone, so the pair is gone with it.
  local twin_allow='^(gc_root_registration\.rs|hal/mod\.rs|pdb/mod\.rs|fs/mod\.rs)$'
  local twins
  twins=$({ comm -12 \
      <(cd "$REPO_ROOT/platforms/rp/src" && find . -name '*.rs' | sed 's|^\./||' | sort) \
      <(cd "$REPO_ROOT/crates/picodroid-core/src" && find . -name '*.rs' | sed 's|^\./||' | sort) \
    | grep -Ev "$twin_allow" || true; })
  [[ -z "$twins" ]] && return 0
  echo ""
  echo "ERROR: these paths exist under BOTH platforms/rp/src and"
  echo "       crates/picodroid-core/src:"
  echo "$twins" | sed 's/^/         /'
  echo ""
  echo "       A file left behind by a move is a shadow twin: both copies"
  echo "       compile, one is dead, and they drift apart silently. Delete"
  echo "       the stale one. If the pair is genuinely two ends of a seam,"
  echo "       add it to twin_allow in scripts/check-source-guards.sh with a"
  echo "       comment saying why."
  return 1
}

check_cfg_gates() {
  # docs/parity-audit.md BLD-02/X2: sim builds keep `family-rp` ACTIVE (the
  # board feature chain), so a gate written as not(feature = "family-rp") does
  # NOT mean "not the simulator" — it selects the no-family path. The
  # existing occurrences are deliberate cross-family gates; a new one usually
  # wants `feature = "sim"` spelled out instead.
  #
  # Was 4 before the shared-core extraction, now 0. Each of the four picked
  # between a real platform capability and a stub — FreeRTOS mutexes vs no-ops
  # (monitor_store x2), a debug-bridge stop poll vs `false` (lvgl/calibration,
  # then native_handler's `interrupted`). Every one of those is a question the
  # RTOS and platform-hook seams now answer, so the gates were deleted rather
  # than moved.
  #
  # The expected count is 0 because that is the end state, not a coincidence:
  # a new occurrence is shared code guessing at the platform instead of asking
  # it. If one is genuinely right, raise the count here and record it under
  # BLD-02 in docs/parity-audit.md.
  # `|| true` because the expected count is now 0: grep exits 1 when it matches
  # nothing, and under `set -euo pipefail` that would abort on the very state
  # this check is supposed to accept.
  local gates
  gates=$({ grep -rn 'not(feature = "family-rp")' \
    "$REPO_ROOT/platforms/rp/src" "$REPO_ROOT/crates/picodroid-core/src" || true; } | wc -l | tr -d ' ')
  [[ "$gates" == "0" ]] && return 0
  grep -rn 'not(feature = "family-rp")' \
    "$REPO_ROOT/platforms/rp/src" "$REPO_ROOT/crates/picodroid-core/src" || true
  echo ""
  echo "ERROR: expected 0 'not(feature = \"family-rp\")' cfg gates,"
  echo "       found $gates. Sim builds activate family-rp, so this"
  echo "       pattern does not exclude the simulator. Prefer a HAL/RTOS/host"
  echo "       seam method, or spell the gate feature = \"sim\". If the new"
  echo "       gate is genuinely correct, bump the expected count in"
  echo "       scripts/check-source-guards.sh and note it under BLD-02 in"
  echo "       docs/parity-audit.md."
  return 1
}

case "${1:-}" in
  --twins)     check_twins ;;
  --cfg-gates) check_cfg_gates ;;
  "")
    rc=0
    check_twins || rc=1
    check_cfg_gates || rc=1
    exit "$rc"
    ;;
  *) echo "Usage: $(basename "$0") [--twins|--cfg-gates]" >&2; exit 2 ;;
esac
