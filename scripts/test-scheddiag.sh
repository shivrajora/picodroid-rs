#!/usr/bin/env bash
# Scheduling-diagnostics soak: run apps under the sched-diag monitor in
# STRICT mode (docs/scheduling-diagnostics.md).
#
# Strict mode turns any HOG / STARVE / POLL / BUSYDELAY / SPIN finding into
# an abort, so a soak that survives its timeout proves nothing spun, hogged
# or starved. To-completion apps must exit cleanly with at least one window
# printed; the detector self-test must print a HOG and, in strict mode,
# abort on it.
#
# Usage:
#   ./scripts/test-scheddiag.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0

# Stop a sim the timeout orphaned. -x (exact name), never -f — a -f pattern
# can match this script's own command line.
kill_sim() { pkill -x picodroid 2>/dev/null || true; }

# Soak an Activity app for a fixed duration under the strict monitor.
# sim.sh compiles before running, so the timeout budgets build time on top
# of the soak; the soak itself is proven by the [schedmon] window count
# (1 window/s), not by wall clock. Surviving until the timeout kill with
# enough windows and no finding = PASS.
soak_test() {
  local name="$1"
  local board="$2"
  local app="$3"
  local secs="$4"
  local min_windows=$((secs / 2))

  echo "==> Soak: $name ($app on $board, ${secs}s + build allowance, strict)"
  local output rc=0
  output=$(PICODROID_SIM_HEADLESS=1 \
           PICODROID_SCHEDDIAG_STRICT=1 \
           timeout $((secs + 180)) \
           bash "$SCRIPT_DIR/sim.sh" --board "$board" --app "$app" --sched-diag 2>&1) || rc=$?
  kill_sim

  # 124/143 = the timeout killed a still-healthy app (the expected soak
  # outcome; 143 when the TERM lands on the sim.sh wrapper). 0 = the app
  # exited by itself first. Anything else (SIGABRT 134 from a strict-mode
  # trip included) is a failure.
  if [[ $rc -ne 0 && $rc -ne 124 && $rc -ne 143 ]]; then
    echo "    FAIL: exit $rc (strict abort or crash)"
    echo "$output" | grep -E "schedmon" | tail -8
    FAIL=$((FAIL + 1))
    return
  fi
  if echo "$output" | grep -qE "\[schedmon\] (HOG|POLL|STARVE|BUSYDELAY|SPIN) "; then
    echo "    FAIL: the monitor reported a finding"
    echo "$output" | grep -E "\[schedmon\] (HOG|POLL|STARVE|BUSYDELAY|SPIN) " | head -3
    FAIL=$((FAIL + 1))
    return
  fi
  local windows
  windows=$(echo "$output" | grep -c "\[schedmon\] w=" || true)
  if [[ $windows -lt $min_windows ]]; then
    echo "    FAIL: only $windows [schedmon] windows (need >= $min_windows) — soak too short"
    echo "$output" | tail -8
    FAIL=$((FAIL + 1))
    return
  fi
  echo "    PASS ($windows windows, no findings)"
  PASS=$((PASS + 1))
}

# Run a to-completion app under the strict monitor: it must exit 0, print
# its own completion pattern, and have printed at least one window.
completion_test() {
  local name="$1"
  local app="$2"
  local pattern="$3"

  echo "==> Completion: $name ($app, strict)"
  local output rc=0
  output=$(PICODROID_SIM_HEADLESS=1 \
           PICODROID_SCHEDDIAG_STRICT=1 \
           timeout 300 \
           bash "$SCRIPT_DIR/sim.sh" --app "$app" --sched-diag 2>&1) || rc=$?
  kill_sim
  if [[ $rc -ne 0 ]]; then
    echo "    FAIL: exit $rc (strict abort or crash)"
    echo "$output" | grep -E "schedmon" | tail -8
    FAIL=$((FAIL + 1))
    return
  fi
  if ! echo "$output" | grep -q "$pattern"; then
    echo "    FAIL: missing '$pattern'"
    echo "$output" | tail -5
    FAIL=$((FAIL + 1))
    return
  fi
  if ! echo "$output" | grep -q "\[schedmon\] w="; then
    echo "    FAIL: no [schedmon] window printed"
    echo "$output" | tail -5
    FAIL=$((FAIL + 1))
    return
  fi
  echo "    PASS"
  PASS=$((PASS + 1))
}

# ── Tests ─────────────────────────────────────────────────────────────────────

# Animation churn: the render path every frame, the timer task every 16 ms.
soak_test "animdemo strict soak" testbench_rp2350 animdemo 20

# The touch kit's clock app: alarm service, ticking UI, the touch sampler.
soak_test "picoclock strict soak" pico_touch_kit picoclock 20

# To-completion apps: a short one; Java threads at the one JVM tier (the
# STARVE rule's territory — with time slicing off, a thread that never
# blocks would starve its peers; threaddemo's tick and tock for a second,
# then the main thread ends the run); and the interpreter benchmark, which
# runs the JVM task flat out for seconds — the shape the POLL and STARVE
# rules must not mistake for a defect.
completion_test "helloworld" helloworld "Hello, World!"
completion_test "threaddemo" threaddemo "tock"
completion_test "benchmark" benchmark "TOTAL:"

# Detector self-test: the timer task holds its core for 5 ms once, which
# the next window MUST report as a HOG — proves the tick-hook path end to
# end, not just that healthy apps stay quiet.
echo "==> Self-test: the injected timer-task hog must print HOG"
rc=0
output=$(PICODROID_SIM_HEADLESS=1 \
         PICODROID_SCHEDDIAG_SELFTEST=1 \
         timeout 60 \
         bash "$SCRIPT_DIR/sim.sh" --app animdemo --sched-diag 2>&1) || rc=$?
kill_sim
if echo "$output" | grep -q "\[schedmon\] HOG Tmr Svc"; then
  echo "    PASS (HOG reported)"
  PASS=$((PASS + 1))
else
  echo "    FAIL: expected '[schedmon] HOG Tmr Svc' (exit $rc)"
  echo "$output" | grep -E "schedmon" | tail -5
  FAIL=$((FAIL + 1))
fi

# The same, strict: the HOG must abort the run.
echo "==> Self-test: strict mode must abort on the injected hog"
rc=0
output=$(PICODROID_SIM_HEADLESS=1 \
         PICODROID_SCHEDDIAG_SELFTEST=1 \
         PICODROID_SCHEDDIAG_STRICT=1 \
         timeout 60 \
         bash "$SCRIPT_DIR/sim.sh" --app animdemo --sched-diag 2>&1) || rc=$?
kill_sim
if echo "$output" | grep -q "\[schedmon\] STRICT: aborting" \
   && [[ $rc -ne 0 && $rc -ne 124 && $rc -ne 143 ]]; then
  echo "    PASS (HOG + strict abort, exit $rc)"
  PASS=$((PASS + 1))
else
  echo "    FAIL: expected a STRICT abort, got exit $rc"
  echo "$output" | grep -E "schedmon" | tail -5
  FAIL=$((FAIL + 1))
fi

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo "==> sched-diag tests: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
