#!/usr/bin/env bash
# Run JVM apps under simulated MCU heap constraints.
#
# The sim allocator (CappedAllocator) enforces a byte limit on the host,
# matching the constrained FreeRTOS heap on RP2040/RP2350.  These tests
# verify that GC and the allocator survive under pressure.
#
# Usage:
#   ./scripts/test-heap.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0

run_test() {
  local name="$1"
  local limit_kb="$2"
  local app="$3"
  local pattern="$4"

  echo "==> Test: $name ($app, ${limit_kb}KB heap limit)"
  local output
  if output=$(PICODROID_HEAP_LIMIT_KB="$limit_kb" timeout 120 \
              bash "$SCRIPT_DIR/sim.sh" --app "$app" 2>&1); then
    if echo "$output" | grep -q "$pattern"; then
      echo "    PASS"
      PASS=$((PASS + 1))
    else
      echo "    FAIL: expected pattern '$pattern' not found"
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

run_test "helloworld@256KB"  256  helloworld  "Hello, World"
run_test "gcstress@256KB"    256  gcstress    "=== PASSED ==="
run_test "heapstress@256KB"  256  heapstress  "=== PASSED ==="
run_test "heapstress@192KB"  192  heapstress  "=== PASSED ==="
run_test "benchmark@256KB"   256  benchmark   "TOTAL:"

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo "==> Heap tests: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
