#!/usr/bin/env bash
# Shared helpers sourced by build.sh, flash.sh, and other scripts.

REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Bench fleet helpers (several boards, one lease each); pure functions.
# shellcheck source=fleet-lib.sh
source "$SCRIPT_DIR/fleet-lib.sh"

# Returns the host target triple (e.g. x86_64-unknown-linux-gnu).
host_target() {
  rustc -vV | awk '/^host:/ { print $2 }'
}

# Prints a timestamped log line to stdout.
timestamp_log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

# Check if all expected patterns are found in a log file.
# Args: log_file "pattern1;pattern2;..."
# Prints missing patterns to stdout; returns 0 if all found, 1 if any missing.
# Print the KEY=VALUE lines of examples/<app>/test.env (comments and blank
# lines dropped), one per line; nothing when the app has no such file. The
# test runners add them to the row's environment: sim-run at run time, hil-run
# at firmware build time (the device reads them through option_env!). For a
# framework switch a conformance app needs on, e.g. reclaimdemo's
# PICODROID_DONT_KEEP_ACTIVITIES=1.
app_test_env() {
  local file="$REPO_ROOT/examples/$1/test.env"
  [[ -f "$file" ]] || return 0
  grep -E '^[A-Za-z_][A-Za-z0-9_]*=' "$file" || true
}

check_patterns() {
  local log_file="$1"
  local patterns="$2"
  local missing=0

  IFS=';' read -ra PATS <<< "$patterns"
  for pat in "${PATS[@]}"; do
    [[ -z "$pat" ]] && continue
    if ! grep -qE "$pat" "$log_file" 2>/dev/null; then
      echo "  MISSING: $pat"
      missing=1
    fi
  done
  return $missing
}

# Scan a log file for crash / panic markers. Positive-pattern matching alone
# can pass a run that emitted the expected line *before* panicking; this
# closes that hole. Prints any markers found; returns 1 if any found.
#
# Markers chosen to be specific enough to avoid false positives on regular log
# output: `panicked` (Rust panic-probe + sim panic banner), `HardFault` (ARM
# Cortex-M fault handler), `SIGSEGV` (sim segfault), `CRASH` uppercase
# (intentional fatal banner).
check_no_crash() {
  local log_file="$1"
  local marker found=0
  for marker in 'panicked' 'HardFault' 'SIGSEGV' 'CRASH'; do
    if grep -qE "$marker" "$log_file" 2>/dev/null; then
      echo "  CRASH MARKER: $marker"
      found=1
    fi
  done
  return $found
}

# The rest of the library, by concern. All four are sourced unconditionally so
# every function stays reachable through `source lib.sh`, as before the split.
# shellcheck source=net-lib.sh
source "$SCRIPT_DIR/net-lib.sh"
# shellcheck source=board-lib.sh
source "$SCRIPT_DIR/board-lib.sh"
# shellcheck source=lock-lib.sh
source "$SCRIPT_DIR/lock-lib.sh"
# shellcheck source=build-lib.sh
source "$SCRIPT_DIR/build-lib.sh"

# Returns the number of logical CPUs (cross-platform: Linux + macOS).
cpu_count() {
  nproc 2>/dev/null || sysctl -n hw.logicalcpu
}

# Prints available app names from the examples directory, one per line, indented.
list_apps() {
  local examples_dir="$1"
  for d in "$examples_dir"/*/; do
    [[ -d "$d" ]] && echo "    $(basename "$d")"
  done
}

# Lists available board names from all platforms/, one per line, indented.
list_boards() {
  for d in "$REPO_ROOT"/platforms/*/boards/*/; do
    [[ -f "$d/board.toml" ]] && echo "    $(basename "$d")"
  done
}

# Prints flash/RAM usage for a given ELF. Requires FLASH_MAX, RAM_MAX, SIZE_TOOL.
# Minimum RAM every firmware image must leave for the core-0 main stack (boot,
# then all core-0 interrupts). See print_memory_usage. 8 KB: the release W
# images ran soaks on 4.7 KB, the debug W image faulted at 4.4 KB. The linker
# script enforces the same number on every build (build_support/flash_layout.rs
# MAIN_STACK_FLOOR_BYTES); keep the two equal.
MAIN_STACK_FLOOR_BYTES=8192
