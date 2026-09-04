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
    *)
      echo "Unknown platform: $PLATFORM" >&2; exit 1
      ;;
  esac

  MANIFEST_DIR="$REPO_ROOT/platforms/$PLATFORM"

  # RP workspace shares the repo-root target/; ESP workspace has its own.
  #
  # CARGO_TARGET_DIR wins when set: pre-commit gives each parallel lane its own
  # build directory (cargo serializes concurrent invocations that share one),
  # and build_firmware looks for the ELF under TARGET_DIR. Hard-pinning this to
  # $REPO_ROOT/target made every such lane report "Binary not found".
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    TARGET_DIR="$CARGO_TARGET_DIR"
  elif [[ "$PLATFORM" == "rp" ]]; then
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

  # Program-image ceiling: LENGTH(FLASH) from the MCU linker script. The
  # image links into the FLASH region, not the whole chip — measuring
  # against total flash hid a 99%-full rp2040 program region behind "43%"
  # (docs/bugs-rp2040-flash-2026-08-01.md, adjacent hazard). Falls back to
  # FLASH_MAX when the script or region is missing (e.g. no linker script).
  # The LENGTH expression ("896K - 0x100") is rewritten into bash arithmetic.
  PROGRAM_FLASH_MAX="$FLASH_MAX"
  local mcu_ld="${mcu_toml%.toml}.x" flash_expr
  if [[ -f "$mcu_ld" ]]; then
    flash_expr=$(grep -E '^[[:space:]]*FLASH[[:space:]]' "$mcu_ld" | head -1 \
      | sed -E 's/.*LENGTH[[:space:]]*=[[:space:]]*//; s|/\*.*||; s/([0-9]+)K/(\1*1024)/g; s/([0-9]+)M/(\1*1048576)/g')
    if [[ -n "$flash_expr" ]]; then
      PROGRAM_FLASH_MAX=$(bash -c "echo \$(( $flash_expr ))" 2>/dev/null) \
        || PROGRAM_FLASH_MAX="$FLASH_MAX"
    fi
  fi

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

  # probe-rs --chip argument for this MCU (HIL scripts). Empty when the MCU
  # has no probe-rs support wired up here; callers must check before use.
  case "$mcu" in
    rp2040) PROBE_CHIP="RP2040" ;;
    rp2350) PROBE_CHIP="RP235x" ;;
    *)      PROBE_CHIP="" ;;
  esac

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

  # Clear first, always. These are `rerun-if-env-changed` inputs to
  # jvm/build.rs, so a value left over from a previous board silently rebuilds
  # pico-jvm (and everything above it) for the next one -- and lints/links that
  # board with the wrong tunables. pico_enviro_mon_w is the only board that
  # sets one (gc_alloc_threshold = 128) and it used to be last in pre-commit's
  # clippy loop, so every stage after it ran with a value no standalone
  # invocation of the same script would have had. The two then took turns
  # invalidating each other's cached pico-jvm in the shared target directory.
  unset PICODROID_JVM_GC_ALLOC_THRESHOLD
  unset PICODROID_JVM_SLOT_CHUNK_SHIFT
  unset PICODROID_JVM_INLINE_ARRAY_DATA

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

# Runs a command holding the repo-wide Gradle lock.
#
# pre-commit fans its stages out across parallel lanes, and two Gradle
# invocations against one project directory contend on Gradle's own project
# lock -- at best blocking, at worst the papk race that produces a
# FrameworkVersionMismatch at `pdb install`. Every gradlew entry point goes
# through here so at most one is ever live.
#
# The timeout is a deadlock detector, not a tuning knob: nothing in this repo
# should hold the lock for ten minutes, and a nested acquisition (a Gradle task
# invoking build-apk.sh without PICODROID_SKIP_GRADLE=1) would otherwise hang
# forever with no clue why. flock is util-linux; without it, run unlocked --
# callers that care run pre-commit --serial.
gradle_lock_run() {
  mkdir -p "$REPO_ROOT/build"
  if command -v flock >/dev/null 2>&1; then
    flock -w 600 "$REPO_ROOT/build/.gradle.lock" "$@"
  else
    "$@"
  fi
}

# Takes the machine-wide lease on the attached dev board for the caller's
# session, or exits 75 (EX_TEMPFAIL) with the holder and a hint.
#
# One probe, one board, several parallel sessions: every script that flashes,
# power-cycles or talks pdb to the board calls this first. If the board is
# free the lease is taken and kept -- it belongs to the *session* (inside
# Claude the claude process, in a terminal the shell that ran the script),
# not to this command, so a flash followed by pdb calls needs no ceremony and
# nothing can interleave. Release with `./scripts/device-lock.sh release`.
# The lease evaporates on its own when the owning process exits.
#
# Optional leading `--wait SECS` queues instead of failing. Remaining args are
# only used to label the lease in `status`.
#
# PICODROID_DEVICE_LOCK=0 skips the check (emergencies). Without flock the
# check is skipped too, matching gradle_lock_run.
require_device_lock() {
  if [[ "${PICODROID_DEVICE_LOCK:-1}" == "0" ]]; then
    echo "WARNING: PICODROID_DEVICE_LOCK=0 -- touching the board without the device lock" >&2
    return 0
  fi
  if ! command -v flock >/dev/null 2>&1; then
    echo "WARNING: flock not found -- touching the board without the device lock" >&2
    return 0
  fi
  local wait_args=()
  if [[ "${1:-}" == "--wait" ]]; then
    wait_args=(--wait "${2:-}")
    shift 2
  fi
  # $PPID in a sourced function is the parent of the script, i.e. the shell
  # (or Claude session) that launched it -- the lease must outlive the script.
  PICODROID_DEVICE_OWNER_PID="${PICODROID_DEVICE_OWNER_PID:-${CLAUDE_PID:-$PPID}}" \
    bash "$SCRIPT_DIR/device-lock.sh" acquire ${wait_args[@]+"${wait_args[@]}"} \
      --note "$(basename "$0") $*" \
    || exit $?
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
# images ran soaks on 4.7 KB, the debug W image faulted at 4.4 KB.
MAIN_STACK_FLOOR_BYTES=8192

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

  printf "  Flash: %d / %d bytes (%d%% of program region; chip total %d)\n" \
    "$flash" "$PROGRAM_FLASH_MAX" "$(( flash * 100 / PROGRAM_FLASH_MAX ))" "$FLASH_MAX"
  printf "  RAM:   %d / %d bytes (%d%%)\n" "$ram" "$RAM_MAX" "$(( ram * 100 / RAM_MAX ))"
  # What .data + .bss leave of RAM is the core-0 main stack: the boot path,
  # then every core-0 interrupt for the life of the firmware (flip-link puts
  # it below .bss, so an overflow runs off the start of RAM and the core
  # locks up before a single log line). Static growth erodes it silently —
  # the network boards were down to 4.4 KB when their debug image stopped
  # booting (2026-09-04) — so a build that leaves less than the floor fails
  # here, in every script that builds firmware, instead of on the board.
  local headroom=$(( RAM_MAX - ram ))
  printf "  Main stack headroom: %d bytes (floor %d)\n" "$headroom" "$MAIN_STACK_FLOOR_BYTES"
  echo ""
  if (( headroom < MAIN_STACK_FLOOR_BYTES )); then
    echo "ERROR: main stack headroom ${headroom} B is below the ${MAIN_STACK_FLOOR_BYTES} B floor" >&2
    echo "       (.data + .bss = ${ram} of ${RAM_MAX} B). Trim static RAM — the heap arena" >&2
    echo "       (mcus/<family>/<mcu>.toml heap_kb) or lv_mem_kb — before this image boots." >&2
    return 1
  fi
}

# Builds the APK and firmware ELF. Sets APK_PATH and ELF as outputs.
# Requires APP, PROFILE, EXTRA_ARGS, BOARD_FEATURE, TARGET, MANIFEST_DIR,
# PACKAGE, TARGET_DIR, and EXTRA_BUILD_ARGS to be set (via resolve_board).
build_firmware() {
  # Line numbers in stack traces — `(File.java:39)` frames instead of
  # `(pc=9)` — ride the `line-numbers` cargo feature plus the
  # LineNumberTable/SourceFile the PAPK and the embedded SDK keep. On for
  # debug-profile firmware (the flash.sh default, where a developer is reading
  # RTT) and off for --release, which HIL, the size ratchet and CI build:
  # the SDK tables alone are ~15 KB of flash on every board
  # (docs/designs/flash-string-budget-2026-08.md §4). PICODROID_LINE_NUMBERS=0|1
  # overrides either way. Resolved before the PAPK build because the PAPK
  # must keep its tables for the same firmware; FIRMWARE_FEATURES is an
  # output so flash.sh's `cargo run` links the identical feature set.
  local lines="${PICODROID_LINE_NUMBERS:-}"
  if [[ -z "$lines" ]]; then
    if [[ "${PROFILE:-debug}" == "release" ]]; then lines=0; else lines=1; fi
  fi
  FIRMWARE_FEATURES="$BOARD_FEATURE${PICODROID_EXTRA_FEATURES:+,$PICODROID_EXTRA_FEATURES}"
  local keep_lines=()
  if [[ "$lines" == "1" ]]; then
    FIRMWARE_FEATURES="$FIRMWARE_FEATURES,line-numbers"
    keep_lines=(--keep-lines)
  fi

  # Step 1: Build the APK for the selected app.
  #
  # PICODROID_PREBUILT_APK short-circuits this. pre-commit builds helloworld
  # once in its serial prologue and points every firmware lane at that one
  # file: without it each lane re-enters Gradle and then copies the result over
  # build/apks/<app>.papk, so concurrent lanes race on the very file the flash
  # gate and the size ratchet measure. Deliberately its own variable rather
  # than PICODROID_APK_PATH, which is set by many callers for other reasons and
  # has never meant "skip the build".
  if [[ -n "${PICODROID_PREBUILT_APK:-}" ]]; then
    APK_PATH="$PICODROID_PREBUILT_APK"
    if [[ ! -f "$APK_PATH" ]]; then
      echo "PICODROID_PREBUILT_APK does not exist: $APK_PATH" >&2
      return 1
    fi
  else
    # The board goes along so the API contract check rejects classes this
    # board excludes from its framework (framework_class_excludes) at build
    # time, not on device. --strip-debug because this PAPK is bound for a
    # device: everything the JVM skips by length is dead flash there.
    # --keep-lines rides along exactly when the firmware gets the
    # line-numbers feature (above). sim.sh builds its own PAPK unstripped.
    bash "$SCRIPT_DIR/build-apk.sh" --app "$APP" --strip-debug \
      ${keep_lines[@]+"${keep_lines[@]}"} ${BOARD:+--board "$BOARD"}
    APK_PATH="$SCRIPT_DIR/../build/apks/${APP}.papk"
  fi

  # Step 2: Build the firmware, embedding the APK.
  local jobs
  jobs=$(cpu_count)
  # Debug-profile FIRMWARE images build with release-grade runtime checks:
  # debug-assertions cost ~37 KB and overflow-checks ~4 KB of flash, which
  # overflows the RP2040's 896K program region. Sim builds (sim.sh, host
  # target) keep both checks — the sim is where invariant debugging happens.
  # HIL builds firmware in --release and is unaffected.
  #
  # Fat LTO (the profile.release default) grows the RP2040 image ~14 KB past
  # that same 896K ceiling — for this codebase LTO inflates the binary rather
  # than shrinking it, so a `--release` link overflows FLASH. Drop LTO for the
  # flash-constrained thumbv6m (RP2040) target so release firmware links; the
  # RP2350 (thumbv8m, 2816K FLASH) keeps fat LTO. This override is a no-op for
  # debug builds, which use profile.dev.
  local flash_gate=()
  if [[ "$TARGET" == thumbv6m* ]]; then
    flash_gate+=(--config 'profile.release.lto=false')
  fi
  # `return`, not a bare command: a caller that invokes build_firmware on the
  # left of `||` runs it with errexit disabled, so a failed cargo used to fall
  # straight through to the ELF check below -- measuring or flashing whatever
  # stale binary the last good build left behind. Report the failure instead.
  # shellcheck disable=SC2086  # CARGO_PLUS is intentionally unquoted (empty or a "+toolchain" override)
  if ! PICODROID_APK_PATH="$APK_PATH" cargo $CARGO_PLUS build \
    --manifest-path "$MANIFEST_DIR/Cargo.toml" \
    --config 'profile.dev.debug-assertions=false' \
    --config 'profile.dev.overflow-checks=false' \
    "${flash_gate[@]}" \
    -p "$PACKAGE" \
    --jobs "$jobs" \
    --target "$TARGET" \
    --no-default-features \
    --features "$FIRMWARE_FEATURES" \
    "${EXTRA_BUILD_ARGS[@]}" \
    "${EXTRA_ARGS[@]}"; then
    echo "cargo build failed: $PACKAGE ($BOARD, $TARGET, $PROFILE)" >&2
    return 1
  fi

  ELF="${TARGET_DIR}/${TARGET}/${PROFILE}/${PACKAGE}"

  # `return` for the same reason as above -- an `exit` here killed the caller
  # outright, which no `||` can intercept.
  if [[ ! -f "$ELF" ]]; then
    echo "Binary not found: $ELF" >&2
    return 1
  fi

  print_memory_usage "$ELF"
}
