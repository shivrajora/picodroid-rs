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

# Sets BOARD_FEATURE, TARGET, FLASH_MAX, RAM_MAX, PLATFORM, PACKAGE, MANIFEST_DIR,
# TARGET_DIR, EXTRA_BUILD_ARGS, and SIZE_TOOL by reading board.toml and mcu.toml.
# Boards are searched across all platforms/ subdirectories.
resolve_board() {
  local board="$1"

  # Search all platforms for this board's board.toml
  local board_toml
  board_toml=$(find "$REPO_ROOT/platforms" -path "*/boards/$board/board.toml" | head -1)

  if [[ -z "$board_toml" ]]; then
    echo "Unknown board: $board" >&2
    echo "Available boards:" >&2
    list_boards >&2
    exit 1
  fi

  # Derive platform from path: platforms/<platform>/boards/...
  PLATFORM=$(echo "$board_toml" | sed "s|$REPO_ROOT/platforms/||" | cut -d/ -f1)

  case "$PLATFORM" in
    rp)
      PACKAGE="picodroid"
      CARGO_PLUS=""        # stable toolchain, no override needed
      ;;
    esp)
      PACKAGE="picodroid-esp"
      CARGO_PLUS="+esp"   # Espressif nightly fork required for -Zbuild-std
      ;;
    *)
      echo "Unknown platform: $PLATFORM" >&2; exit 1
      ;;
  esac

  MANIFEST_DIR="$REPO_ROOT/platforms/$PLATFORM"

  # RP workspace shares the repo-root target/; ESP workspace has its own.
  if [[ "$PLATFORM" == "rp" ]]; then
    TARGET_DIR="$REPO_ROOT/target"
  else
    TARGET_DIR="$MANIFEST_DIR/target"
  fi

  # Board feature name: underscores → hyphens for Cargo
  BOARD_FEATURE="board-$(echo "$board" | tr '_' '-')"

  # Read MCU name from board.toml
  local mcu
  mcu=$(grep '^mcu' "$board_toml" | sed 's/.*= *"\{0,1\}\([^"]*\)"\{0,1\}/\1/' | tr -d ' ')

  # Find mcu.toml across all platforms
  local mcu_toml
  mcu_toml=$(find "$REPO_ROOT/platforms" -name "${mcu}.toml" 2>/dev/null | head -1)
  if [[ -z "$mcu_toml" ]]; then
    echo "MCU definition not found: ${mcu}.toml under platforms/" >&2
    exit 1
  fi

  TARGET=$(grep '^target' "$mcu_toml" | sed 's/.*= *"\{0,1\}\([^"]*\)"\{0,1\}/\1/' | tr -d ' ')
  local ram_kb flash_kb
  ram_kb=$(grep '^ram_kb' "$mcu_toml" | sed 's/.*= *//' | tr -d ' ')
  flash_kb=$(grep '^flash_kb' "$mcu_toml" | sed 's/.*= *//' | tr -d ' ')
  RAM_MAX=$(( ram_kb * 1024 ))
  FLASH_MAX=$(( flash_kb * 1024 ))

  # Optional extra cargo flags (e.g. -Zbuild-std=core,alloc for ESP nightly builds).
  # Guard with grep -q to avoid failing under set -e when the key is absent.
  # Use [^=]*= (not .*=) so the sed strips only up to the FIRST '=', preserving
  # any '=' signs that appear inside the value (e.g. -Zbuild-std=core,alloc).
  EXTRA_BUILD_ARGS=()
  if grep -q '^extra_build_args' "$mcu_toml" 2>/dev/null; then
    local raw_extra
    raw_extra=$(grep '^extra_build_args' "$mcu_toml" | sed 's/^[^=]*= *//' | tr -d '"')
    IFS=' ' read -ra EXTRA_BUILD_ARGS <<< "$raw_extra"
  fi

  SIZE_TOOL="arm-none-eabi-size"
  if grep -q '^size_tool' "$mcu_toml" 2>/dev/null; then
    SIZE_TOOL=$(grep '^size_tool' "$mcu_toml" | sed 's/^[^=]*= *//' | tr -d '"')
  fi

  apply_jvm_env "$board_toml"
}

# Export PICODROID_JVM_* env vars from board.toml's optional `[jvm]` section
# so the `pico-jvm` crate's build.rs (which runs before the platform crate
# and so can't see board.toml directly) can pick them up as `pub const`
# tunables. Keys present in `[jvm]` are exported; missing keys fall back to
# the hardcoded defaults baked into `jvm/build.rs`.
#
# Canonical guide: website/src/content/docs/reference/jvm-tunables.md.
# Schema enforcement: platforms/rp/build.rs::emit_jvm_config.
apply_jvm_env() {
  local board_toml="$1"
  # Extract the [jvm] block: from "[jvm]" up to the next "[" line, or EOF.
  local block
  block=$(awk '
    /^\[jvm\]/ { in_block=1; next }
    in_block && /^\[/ { exit }
    in_block { print }
  ' "$board_toml")
  [[ -z "$block" ]] && return 0

  _export_jvm_kv "$block" "gc_alloc_threshold" PICODROID_JVM_GC_ALLOC_THRESHOLD
  _export_jvm_kv "$block" "slot_chunk_shift"   PICODROID_JVM_SLOT_CHUNK_SHIFT
  _export_jvm_kv "$block" "inline_array_data"  PICODROID_JVM_INLINE_ARRAY_DATA
  # NOTE: activity_stack_depth and pending_op_queue are consumed by
  # platforms/rp/build.rs directly via the parsed BoardConfig, so they don't
  # need env-var plumbing.
}

# Helper: if $block has "<key> = <value>", export NAME=value.
# Strips inline comments and surrounding whitespace. No-op when key absent.
#
# The `|| true` swallows pipefail when `grep` finds no match — a `[jvm]`
# block that sets some keys but not others is a legitimate partial override,
# and without this guard `set -e` would terminate the caller.
_export_jvm_kv() {
  local block="$1" key="$2" name="$3" value
  value=$(echo "$block" | grep -E "^\s*$key\s*=" 2>/dev/null | head -1 \
    | sed -E "s/^\s*$key\s*=\s*//; s/#.*$//; s/\s+$//" || true)
  [[ -z "$value" ]] && return 0
  export "$name=$value"
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

# Lists available board names from all platforms/, one per line, indented.
list_boards() {
  for d in "$REPO_ROOT"/platforms/*/boards/*/; do
    [[ -f "$d/board.toml" ]] && echo "    $(basename "$d")"
  done
}

# Prints flash/RAM usage for a given ELF. Requires FLASH_MAX, RAM_MAX, SIZE_TOOL.
print_memory_usage() {
  local elf="$1"
  if ! command -v "$SIZE_TOOL" &>/dev/null; then
    echo "(skipping memory usage: $SIZE_TOOL not found)"
    return
  fi
  local size_output
  size_output=$("$SIZE_TOOL" "$elf")
  echo ""
  echo "=== Memory Usage ==="
  echo "$size_output"

  read -r TEXT DATA BSS <<< "$(echo "$size_output" | awk 'NR==2 {print $1, $2, $3}')"
  local flash=$(( TEXT + DATA ))
  local ram=$(( DATA + BSS ))

  printf "  Flash: %d / %d bytes (%d%%)\n" "$flash" "$FLASH_MAX" "$(( flash * 100 / FLASH_MAX ))"
  printf "  RAM:   %d / %d bytes (%d%%)\n" "$ram" "$RAM_MAX" "$(( ram * 100 / RAM_MAX ))"
  echo ""
}

# Builds the APK and firmware ELF. Sets APK_PATH and ELF as outputs.
# Requires APP, PROFILE, EXTRA_ARGS, BOARD_FEATURE, TARGET, MANIFEST_DIR,
# PACKAGE, TARGET_DIR, and EXTRA_BUILD_ARGS to be set (via resolve_board).
build_firmware() {
  # Step 1: Build the APK for the selected app.
  bash "$SCRIPT_DIR/build-apk.sh" --app "$APP"

  APK_PATH="$SCRIPT_DIR/../build/apks/${APP}.papk"

  # Step 2: Build the firmware, embedding the APK.
  local jobs
  jobs=$(cpu_count)
  # Debug-profile FIRMWARE images build with release-grade runtime checks:
  # debug-assertions cost ~37 KB and overflow-checks ~4 KB of flash, which
  # overflows the RP2040's 896K program region. Sim builds (sim.sh, host
  # target) keep both checks — the sim is where invariant debugging happens.
  # HIL builds firmware in --release and is unaffected.
  # shellcheck disable=SC2086  # CARGO_PLUS is intentionally unquoted (empty or "+esp")
  PICODROID_APK_PATH="$APK_PATH" cargo $CARGO_PLUS build \
    --manifest-path "$MANIFEST_DIR/Cargo.toml" \
    --config 'profile.dev.debug-assertions=false' \
    --config 'profile.dev.overflow-checks=false' \
    -p "$PACKAGE" \
    --jobs "$jobs" \
    --target "$TARGET" \
    --no-default-features \
    --features "$BOARD_FEATURE" \
    "${EXTRA_BUILD_ARGS[@]}" \
    "${EXTRA_ARGS[@]}"

  ELF="${TARGET_DIR}/${TARGET}/${PROFILE}/${PACKAGE}"

  if [[ ! -f "$ELF" ]]; then
    echo "Binary not found: $ELF" >&2
    exit 1
  fi

  print_memory_usage "$ELF"
}
