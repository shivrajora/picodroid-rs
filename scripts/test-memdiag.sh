#!/usr/bin/env bash
# Memory-diagnostics soak: run apps under the mem-diag monitor with the
# growth sentinel in STRICT mode (docs/memory-diagnostics.md).
#
# Strict mode turns steady-state heap growth into a hard abort, so a soak
# that survives its timeout proves the live floor stayed flat. Non-Activity
# apps run to completion and are checked for a clean [memmon] snapshot.
#
# Usage:
#   ./scripts/test-memdiag.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0

# Soak an Activity app for a fixed duration under the strict sentinel.
# sim.sh compiles before running, so the timeout budgets build time on top
# of the soak; the soak itself is proven by the [memmon] window count
# (1 window/s), not by wall clock. Surviving until the timeout kill with
# enough windows, no LEAK?, and no strict abort = PASS.
soak_test() {
  local name="$1"
  local board="$2"
  local app="$3"
  local secs="$4"
  local min_windows=$((secs / 2))

  echo "==> Soak: $name ($app on $board, ${secs}s + build allowance, strict sentinel)"
  local output rc=0
  output=$(PICODROID_SIM_HEADLESS=1 \
           PICODROID_MEMDIAG_SENTINEL=1 \
           PICODROID_MEMDIAG_STRICT=1 \
           timeout $((secs + 180)) \
           bash "$SCRIPT_DIR/sim.sh" --board "$board" --app "$app" --mem-diag 2>&1) || rc=$?
  # timeout TERMs the sim.sh wrapper; the cargo-run sim binary can survive
  # orphaned. -x (exact name), never -f — a -f pattern can match this
  # script's own command line.
  pkill -x picodroid 2>/dev/null || true

  # 124/143 = the timeout killed a still-healthy app (the expected soak
  # outcome; 143 when the TERM lands on the sim.sh wrapper). 0 = the app
  # exited by itself first (fine for short demos). Anything else (SIGABRT
  # 134 from a strict-mode trip included) is a failure.
  if [[ $rc -ne 0 && $rc -ne 124 && $rc -ne 143 ]]; then
    echo "    FAIL: exit $rc (strict sentinel abort or crash)"
    echo "$output" | tail -8
    FAIL=$((FAIL + 1))
    return
  fi
  if echo "$output" | grep -q "LEAK?"; then
    echo "    FAIL: sentinel reported a leak"
    echo "$output" | grep "LEAK?" | head -3
    FAIL=$((FAIL + 1))
    return
  fi
  local windows
  windows=$(echo "$output" | grep -c "\[memmon\] w=" || true)
  if [[ $windows -lt $min_windows ]]; then
    echo "    FAIL: only $windows [memmon] windows (need >= $min_windows) — soak too short"
    echo "$output" | tail -8
    FAIL=$((FAIL + 1))
    return
  fi
  echo "    PASS ($windows windows, flat floor)"
  PASS=$((PASS + 1))
}

# Run a to-completion app and require the final [memmon] snapshot.
snapshot_test() {
  local name="$1"
  local app="$2"
  local pattern="$3"

  echo "==> Snapshot: $name ($app)"
  local output
  if output=$(PICODROID_SIM_HEADLESS=1 timeout 120 \
              bash "$SCRIPT_DIR/sim.sh" --app "$app" --mem-diag 2>&1); then
    if echo "$output" | grep -q "$pattern" \
       && echo "$output" | grep -q "\[memmon\] snapshot"; then
      echo "    PASS"
      PASS=$((PASS + 1))
    else
      echo "    FAIL: missing '$pattern' or [memmon] snapshot line"
      echo "$output" | tail -5
      FAIL=$((FAIL + 1))
    fi
  else
    echo "    FAIL: non-zero exit"
    echo "$output" | tail -5
    FAIL=$((FAIL + 1))
  fi
}

# ── Tests ─────────────────────────────────────────────────────────────────────

# Animation churn exercises the render/timer path every frame; 30 s covers
# ~28 monitor windows — enough for the K=8 sentinel to trip on real growth.
soak_test "animdemo strict soak" testbench_rp2350 animdemo 30

# Offensive-mode soak: poison-on-free + GC poison check + integrity sweep +
# allocator canaries all armed. gcstress runs to completion (=== PASSED ===)
# under 324+ GC cycles of poisoned frees; any violation aborts.
echo "==> Offensive: gcstress under PICODROID_MEMDIAG_OFFENSIVE"
if output=$(PICODROID_SIM_HEADLESS=1 PICODROID_MEMDIAG_OFFENSIVE=1 timeout 120 \
            bash "$SCRIPT_DIR/sim.sh" --app gcstress --mem-diag 2>&1) \
   && echo "$output" | grep -q "=== PASSED ==="; then
  echo "    PASS"
  PASS=$((PASS + 1))
else
  echo "    FAIL"
  echo "$output" | tail -8
  FAIL=$((FAIL + 1))
fi

snapshot_test "benchmark snapshot" benchmark "TOTAL:"

# Detector self-test: a synthetic +2 KB/window ramp MUST trip the sentinel
# (LEAK? line) and strict mode MUST abort — proves the detection path
# end-to-end, not just that healthy apps stay quiet.
echo "==> Self-test: sentinel must trip on the synthetic ramp"
rc=0
output=$(PICODROID_SIM_HEADLESS=1 \
         PICODROID_MEMDIAG_SENTINEL=1 \
         PICODROID_MEMDIAG_STRICT=1 \
         PICODROID_MEMDIAG_SELFTEST=1 \
         timeout 240 \
         bash "$SCRIPT_DIR/sim.sh" --app animdemo --mem-diag 2>&1) || rc=$?
pkill -x picodroid 2>/dev/null || true
if echo "$output" | grep -q "LEAK?" && [[ $rc -ne 0 && $rc -ne 124 && $rc -ne 143 ]]; then
  echo "    PASS (tripped + strict abort, exit $rc)"
  PASS=$((PASS + 1))
else
  echo "    FAIL: expected LEAK? + abort, got exit $rc"
  echo "$output" | grep -E "LEAK\?|memmon" | tail -5
  FAIL=$((FAIL + 1))
fi

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo "==> mem-diag tests: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
