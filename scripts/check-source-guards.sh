#!/usr/bin/env bash
# Source-tree invariants that are pure text checks: no compiler, seconds long.
# Shared by scripts/pre-commit (its `twins` and `cfg_gates` stages) and the CI
# `guards` job, so a bypassed hook (--no-verify, a merge) still meets them on
# the server.
#
#   ./scripts/check-source-guards.sh                  # every check
#   ./scripts/check-source-guards.sh --twins          # shadow twins only
#   ./scripts/check-source-guards.sh --cfg-gates      # cfg-gate hygiene only
#   ./scripts/check-source-guards.sh --unsafe         # unsafe ratchet only
#   ./scripts/check-source-guards.sh --unsafe-accept  # rewrite its baseline
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
  # Every workspace crate, not only picodroid-core: code lifted out of core
  # into a crate of its own (pd-drivers, pd-install, ...) is still shared code.
  local gates
  gates=$({ grep -rn --include='*.rs' 'not(feature = "family-rp")' \
    "$REPO_ROOT/platforms/rp/src" "$REPO_ROOT/crates" || true; } | wc -l | tr -d ' ')
  [[ "$gates" == "0" ]] && return 0
  grep -rn --include='*.rs' 'not(feature = "family-rp")' \
    "$REPO_ROOT/platforms/rp/src" "$REPO_ROOT/crates" || true
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

UNSAFE_BASELINE="$REPO_ROOT/scripts/unsafe-baseline.conf"

# One line per workspace crate: `<crate dir> <blocks> <fns> <impls>
# <static mut> <undocumented>`. Text counts over non-comment lines, so they
# move with the code and with nothing else:
#   blocks        `unsafe {`
#   fns           `unsafe fn`, `unsafe extern "C" fn`
#   impls         `unsafe impl`
#   static mut    `static mut`
#   undocumented  `unsafe {` with no SAFETY on its line or the three above
unsafe_counts() {
  local dir
  for dir in "$REPO_ROOT"/crates/*/ "$REPO_ROOT"/platforms/*/ "$REPO_ROOT"/tools/*/; do
    [[ -f "$dir/Cargo.toml" ]] || continue
    dir="${dir%/}"
    find "$dir" -name target -prune -o -name '*.rs' -print0 | sort -z \
      | xargs -0 awk -v crate="${dir#"$REPO_ROOT"/}" '
          FNR == 1 { a = b = c = "" }
          {
            line = $0
            if (line !~ /^[[:space:]]*\/\//) {
              n = gsub(/unsafe[[:space:]]*\{/, "&", line)
              blocks += n
              if (n > 0 && (a b c $0) !~ /SAFETY/) undoc += n
              fns += gsub(/unsafe[[:space:]]+(extern[[:space:]]+"[A-Za-z]+"[[:space:]]+)?fn[[:space:]]/, "&", line)
              impls += gsub(/unsafe[[:space:]]+impl[ <]/, "&", line)
              smut += gsub(/static[[:space:]]+mut[[:space:]]/, "&", line)
            }
            a = b; b = c; c = $0
          }
          END { printf "%s %d %d %d %d %d\n", crate, blocks, fns, impls, smut, undoc }'
  done
}

check_unsafe() {
  # The count of `unsafe` only goes down (docs/designs/unsafe-reduction-2026-09.md).
  # A rise fails; so does a fall the baseline has not caught up with, the way
  # the binary-size ratchet does, so the floor is always the tree's own.
  local current
  current=$(unsafe_counts)
  if [[ "${1:-}" == "accept" ]]; then
    {
      echo "# Per-crate counts of unsafe Rust; scripts/check-source-guards.sh --unsafe."
      echo "# crate blocks fns impls static_mut undocumented"
      echo "$current"
    } > "$UNSAFE_BASELINE"
    echo "wrote ${UNSAFE_BASELINE#"$REPO_ROOT"/}"
    return 0
  fi
  [[ -f "$UNSAFE_BASELINE" ]] || { echo "ERROR: $UNSAFE_BASELINE is missing"; return 1; }
  local report
  report=$(awk -v names="blocks fns impls static_mut undocumented" '
      BEGIN { split(names, name, " ") }
      NR == FNR { if ($0 !~ /^#/ && NF == 6) { base[$1] = $0 }; next }
      {
        seen[$1] = 1
        if (!($1 in base)) { printf "new   %s: not in the baseline\n", $1; next }
        split(base[$1], was, " ")
        for (i = 2; i <= 6; i++) {
          if ($i > was[i]) printf "rise  %s: %s %d -> %d\n", $1, name[i - 1], was[i], $i
          if ($i < was[i]) printf "fall  %s: %s %d -> %d\n", $1, name[i - 1], was[i], $i
        }
      }
      END { for (k in base) if (!(k in seen)) printf "gone  %s: still in the baseline\n", k }
    ' "$UNSAFE_BASELINE" <(echo "$current"))
  [[ -z "$report" ]] && return 0
  echo ""
  echo "$report" | sort | sed 's/^/         /'
  echo ""
  if echo "$report" | grep -q '^rise'; then
    echo "ERROR: more unsafe Rust than scripts/unsafe-baseline.conf allows."
    echo "       Reach for a shared primitive first (picodroid-core's util/,"
    echo "       a HAL or RTOS seam method). If the new unsafe is genuinely"
    echo "       needed, give it a SAFETY comment and raise the baseline in"
    echo "       the same commit: ./scripts/check-source-guards.sh --unsafe-accept"
  else
    echo "ERROR: scripts/unsafe-baseline.conf is behind the tree. Lower it in"
    echo "       the same commit: ./scripts/check-source-guards.sh --unsafe-accept"
  fi
  return 1
}

case "${1:-}" in
  --twins)         check_twins ;;
  --cfg-gates)     check_cfg_gates ;;
  --unsafe)        check_unsafe ;;
  --unsafe-accept) check_unsafe accept ;;
  "")
    rc=0
    check_twins || rc=1
    check_cfg_gates || rc=1
    check_unsafe || rc=1
    exit "$rc"
    ;;
  *) echo "Usage: $(basename "$0") [--twins|--cfg-gates|--unsafe|--unsafe-accept]" >&2; exit 2 ;;
esac
