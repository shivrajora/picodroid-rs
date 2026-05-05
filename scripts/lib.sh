#!/usr/bin/env bash
# Shared helpers sourced by build.sh, flash.sh, and other scripts.

REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

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

# Auto-detect the USB hub location by finding the hub with a CMSIS-DAP probe.
detect_usb_hub() {
  sudo uhubctl 2>/dev/null | awk '/^Current status for hub/{hub=$5} /CMSIS-DAP/{print hub}'
}

# Sets BOARD_FEATURE, TARGET, FLASH_MAX, RAM_MAX by reading board.toml and mcu.toml.
# RP boards live under platforms/rp/boards/; MCU definitions under platforms/rp/mcus/.
resolve_board() {
  local board="$1"
  local board_toml="$REPO_ROOT/platforms/rp/boards/$board/board.toml"

  if [[ ! -f "$board_toml" ]]; then
    echo "Unknown board: $board" >&2
    echo "Available boards:" >&2
    for d in "$REPO_ROOT"/platforms/rp/boards/*/; do
      [[ -f "$d/board.toml" ]] && echo "  $(basename "$d")"
    done
    exit 1
  fi

  # Board feature name: underscores → hyphens for Cargo
  BOARD_FEATURE="board-$(echo "$board" | tr '_' '-')"

  # Read MCU name from board.toml
  local mcu
  mcu=$(grep '^mcu' "$board_toml" | sed 's/.*= *"\{0,1\}\([^"]*\)"\{0,1\}/\1/' | tr -d ' ')

  # Find mcu.toml under platforms/rp/mcus/
  local mcu_toml
  mcu_toml=$(find "$REPO_ROOT/platforms/rp/mcus" -name "${mcu}.toml" 2>/dev/null | head -1)
  if [[ -z "$mcu_toml" ]]; then
    echo "MCU definition not found: platforms/rp/mcus/*/${mcu}.toml" >&2
    exit 1
  fi

  # Read target triple, RAM, flash from mcu.toml
  TARGET=$(grep '^target' "$mcu_toml" | sed 's/.*= *"\{0,1\}\([^"]*\)"\{0,1\}/\1/' | tr -d ' ')
  local ram_kb flash_kb
  ram_kb=$(grep '^ram_kb' "$mcu_toml" | sed 's/.*= *//' | tr -d ' ')
  flash_kb=$(grep '^flash_kb' "$mcu_toml" | sed 's/.*= *//' | tr -d ' ')
  RAM_MAX=$(( ram_kb * 1024 ))
  FLASH_MAX=$(( flash_kb * 1024 ))
}

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

# Lists available RP board names from platforms/rp/boards/, one per line, indented.
list_boards() {
  for d in "$REPO_ROOT"/platforms/rp/boards/*/; do
    [[ -f "$d/board.toml" ]] && echo "    $(basename "$d")"
  done
}

# Prints flash/RAM usage for a given ELF. Requires FLASH_MAX and RAM_MAX to be set.
print_memory_usage() {
  local elf="$1"
  local size_output
  size_output=$(arm-none-eabi-size "$elf")
  echo ""
  echo "=== Memory Usage ==="
  echo "$size_output"

  read TEXT DATA BSS <<< $(echo "$size_output" | awk 'NR==2 {print $1, $2, $3}')
  local flash=$(( TEXT + DATA ))
  local ram=$(( DATA + BSS ))

  printf "  Flash: %d / %d bytes (%d%%)\n" "$flash" "$FLASH_MAX" "$(( flash * 100 / FLASH_MAX ))"
  printf "  RAM:   %d / %d bytes (%d%%)\n" "$ram" "$RAM_MAX" "$(( ram * 100 / RAM_MAX ))"
  echo ""
}

# Builds the APK and firmware ELF. Sets APK_PATH and ELF as outputs.
# Requires BOARD, APP, PROFILE, EXTRA_ARGS, BOARD_FEATURE, and TARGET to be set.
build_firmware() {
  # Step 1: Build the APK for the selected app.
  bash "$SCRIPT_DIR/build-apk.sh" --app "$APP"

  APK_PATH="$SCRIPT_DIR/../build/apks/${APP}.papk"

  # Step 2: Build the firmware, embedding the APK.
  local jobs
  jobs=$(cpu_count)
  PICODROID_APK_PATH="$APK_PATH" cargo build \
    -p picodroid \
    --jobs "$jobs" \
    --target "$TARGET" \
    --no-default-features \
    --features "$BOARD_FEATURE" \
    "${EXTRA_ARGS[@]}"

  ELF="target/${TARGET}/${PROFILE}/picodroid"

  if [[ ! -f "$ELF" ]]; then
    echo "Binary not found: $ELF" >&2
    exit 1
  fi

  print_memory_usage "$ELF"
}
