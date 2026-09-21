#!/usr/bin/env bash
# The bench and the board: USB hub / power cycling, debug-probe pinning,
# board resolution, board.toml lookups, and the `[jvm]` -> PICODROID_JVM_* env.
#
# Sourced by lib.sh (which sets REPO_ROOT and SCRIPT_DIR); do not run or
# source directly. Functions only -- no work happens at source time.

# Auto-detect the USB hub location by finding the hub with a CMSIS-DAP probe.
detect_usb_hub() {
  sudo uhubctl 2>/dev/null | awk '/^Current status for hub/{hub=$5} /CMSIS-DAP/{print hub}' | sort -u
}

# Power-cycles the bench for the current slot. With a fleet
# (PICODROID_SLOT, exported by require_device_lock) that is the slot's probe
# port and board port only -- fleet-lib.sh::power_cycle_slot -- so the other
# boards keep running. Without a fleet it is every port of the hub that
# carries the CMSIS-DAP probe, as it always was. Prints the uhubctl commands.
power_cycle_bench() {
  if [[ -n "${PICODROID_SLOT:-}" ]]; then
    power_cycle_slot "$PICODROID_SLOT"
    return
  fi
  local hub
  hub=$(detect_usb_hub | head -1)
  if [[ -z "$hub" ]]; then
    echo "power cycle: no USB hub with a CMSIS-DAP probe detected" >&2
    return 1
  fi
  echo "uhubctl -l $hub -a cycle"
  sudo uhubctl -l "$hub" -a cycle
}

# Pins probe-rs to the bench's CMSIS-DAP debug probe. With a second probe
# enumerated (an STLink left on the hub, 2026-09-05..07 nightlies) probe-rs
# prompts "Selection:" on stdin and every run/reset/flash dies with "Failed
# to parse probe index" -- and inside hil-run's config loop the prompt eats
# the config file, so the remaining rows come back as garbage app names.
# Exports PROBE_RS_PROBE as VID:PID:SERIAL (the form --probe accepts; every
# probe-rs subcommand reads the variable). A value already in the
# environment wins, so an operator can point at another probe. No-op when
# no CMSIS-DAP probe is enumerated (the caller's own wait/skip logic decides
# what that means). Prints the selector it pinned, nothing otherwise.
pin_debug_probe() {
  [[ -n "${PROBE_RS_PROBE:-}" ]] && return 0
  command -v probe-rs >/dev/null 2>&1 || return 0
  local selector
  # `probe-rs list` prints "... -- 2e8a:000c-0:E663...  (CMSIS-DAP)"; the
  # "-0" after the PID is the USB interface, which --probe does not take.
  selector=$(probe-rs list 2>/dev/null \
    | awk '/CMSIS-DAP/ { for (i = 1; i <= NF; i++) if ($i ~ /^[0-9a-fA-F]{4}:[0-9a-fA-F]{4}/) { print $i; exit } }' \
    | sed -E 's/^([0-9a-fA-F]{4}:[0-9a-fA-F]{4})(-[0-9]+)?:/\1:/')
  [[ -n "$selector" ]] || return 0
  export PROBE_RS_PROBE="$selector"
  echo "$selector"
}

# Sets BOARD_FEATURE, TARGET, MCU, FLASH_MAX, RAM_MAX, PLATFORM, PACKAGE, MANIFEST_DIR,
# TARGET_DIR, EXTRA_BUILD_ARGS, PROBE_CHIP and SIZE_TOOL by reading board.toml and mcu.toml.
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
  MCU="$mcu"

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

  # Program-image ceiling: what the generated memory.x gives the FLASH
  # region. The image links into that region, not the whole chip — measuring
  # against total flash hid a 99%-full rp2040 program region behind "43%"
  # (docs/bugs-rp2040-flash-2026-08-01.md, adjacent hazard). The region is
  # laid out top-down from the end of flash by build_support/flash_layout.rs
  # (boot2 in front, then the program image, LittleFS, the app region); the
  # same subtraction over the same keys keeps this gate and the linker in
  # step. board.toml overrides the MCU defaults for the tunable keys.
  local boot2 fs_kb region_kb
  boot2=$(toml_top_int "$mcu_toml" boot2_bytes 0)
  fs_kb=$(toml_top_int "$board_toml" fs_kb "$(toml_top_int "$mcu_toml" fs_kb 0)")
  region_kb=$(toml_top_int "$board_toml" app_region_kb "$(toml_top_int "$mcu_toml" app_region_kb 0)")
  MAX_INSTALLED_APPS=$(toml_top_int "$board_toml" max_installed_apps "$(toml_top_int "$mcu_toml" max_installed_apps 1)")
  APP_REGION_KB="$region_kb"
  PROGRAM_FLASH_MAX=$(( FLASH_MAX - boot2 - fs_kb * 1024 - region_kb * 1024 ))

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
    # probe-rs has one target for both RP2350 variants.
    rp2350|rp2350b) PROBE_CHIP="RP235x" ;;
    *)      PROBE_CHIP="" ;;
  esac

  apply_jvm_env "$board_toml"
}

# Reads an integer top-level key (before the first [section]) from a toml
# file, or prints the default when absent. Strips quotes and a trailing
# comment; hex (0x100) passes through bash arithmetic unchanged.
toml_top_int() {
  local file="$1" key="$2" default="$3" value
  value=$(awk -v k="$key" '
    /^[[:space:]]*\[/ { exit }
    $0 ~ "^[[:space:]]*" k "[[:space:]]*=" {
      sub(/^[^=]*=[[:space:]]*/, ""); sub(/[[:space:]]*#.*$/, ""); gsub(/"/, ""); print; exit
    }' "$file" 2>/dev/null)
  [[ -z "$value" ]] && value="$default"
  echo $(( value ))
}

# Where the settings app's uninstall dialog puts its positive button, as
# `x y` in display pixels for `input tap`.
#
# The dialog is a 200x160 card centred on the display, with two 80 px buttons
# side by side in a row 78 px down from the card's top edge
# (`lvgl/widgets/alert_dialog.rs`: card_y = max((screen_h - 160) / 2, 8), the
# flex column ends in a 50 px button row). So the positive button sits 40 px
# right of the centre line and 78 px into the card — 160,118 on a 240x240
# board, 200,118 on the 320x240 testbench, 200,238 on a 320x480 panel.
#
# Both coordinates are derived. `y` was a literal 118 until pico_touch_kit
# arrived: every board with this lane was 240 px tall, so a constant and a
# centred card were indistinguishable, and the constant taps the message text
# on a 480 px panel.
settings_dialog_ok() {
  local board="$1" board_toml width height card_y
  board_toml=$(find "$REPO_ROOT/platforms" -path "*/boards/$board/board.toml" | head -1)
  width=$(display_dim "$board_toml" width)
  height=$(display_dim "$board_toml" height)
  card_y=$(( (${height:-240} - 160) / 2 ))
  (( card_y < 8 )) && card_y=8
  # The button row is the card's third flex child (12 px pad, two labels, the
  # theme's row gaps, 6 px row pad): its 36 px buttons span roughly card_y+72
  # to card_y+108, so aim at the middle. The old +78 sat on the top edge and
  # missed on the touch kit (nightly 2026-09-15: +78 missed, +90 uninstalled).
  echo "$(( ${width:-240} / 2 + 40 )) $(( card_y + 90 ))"
}

# One `[display]` integer key from a board.toml, empty when absent.
display_dim() {
  awk -F= -v key="$2" '
    /^\[display\]/ { d = 1; next }
    /^\[/          { d = 0 }
    d && $1 ~ "^" key "[[:space:]]*$" { gsub(/[[:space:]]/, "", $2); print $2; exit }
  ' "$1" 2>/dev/null
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
