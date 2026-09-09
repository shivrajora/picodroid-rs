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

# ── Host-side networking for `net` rows (hil-run.sh, sim-run.sh) ───────────
#
# The `net` rows in hil-tests.conf (netdemo, http_get) need two servers on the
# test host: a TCP echo on port 7000 and an HTTP server on port 8000. The
# runners start them just before the first `net` row and stop them on exit.

NET_ECHO_PORT=7000
NET_HTTP_PORT=8000
NET_LISTENER_PIDS=()

# Prints this machine's LAN IPv4 address (the one a board on the same network
# reaches). Prints nothing on failure. The address is DHCP-assigned, so it is
# looked up on every run rather than hardcoded.
host_lan_ip() {
  ip -4 route get 1.1.1.1 2>/dev/null | grep -oP 'src \K[\d.]+' | head -1
}

# True when something accepts TCP connections on 127.0.0.1:<port>.
net_port_open() {
  local port="$1"
  (exec 3<> "/dev/tcp/127.0.0.1/$port") 2>/dev/null
}

# Wait up to <secs> for a port to accept connections. Returns 1 on timeout.
net_wait_port() {
  local port="$1" secs="${2:-5}" i=0
  while (( i < secs * 10 )); do
    net_port_open "$port" && return 0
    sleep 0.1
    i=$((i + 1))
  done
  return 1
}

# Start the echo and HTTP servers in the background.
# Args: log_dir — where net-echo.log / net-http.log go.
# Records the PIDs in NET_LISTENER_PIDS; a second call is a no-op. Returns 1
# with the reason in NET_LISTENER_ERR when socat/python3 are missing, a port
# is already taken by someone else, or a server does not come up within 5 s.
# Call it directly, never inside `$(...)`: a command substitution runs in a
# subshell, so the PIDs would be lost and the servers would leak.
NET_LISTENER_ERR=""
start_net_listeners() {
  local log_dir="$1"
  NET_LISTENER_ERR=""
  [[ ${#NET_LISTENER_PIDS[@]} -gt 0 ]] && return 0

  # hil-fleet.sh runs the two servers once for all its runners (the ports
  # are host-global); a runner then only checks they are up and stops
  # nothing (NET_LISTENER_PIDS stays empty).
  if [[ "${PICODROID_NET_LISTENERS_EXTERNAL:-0}" == "1" ]]; then
    local port
    for port in "$NET_ECHO_PORT" "$NET_HTTP_PORT"; do
      if ! net_wait_port "$port" 5; then
        NET_LISTENER_ERR="net listeners: PICODROID_NET_LISTENERS_EXTERNAL=1 but nothing listens on port $port"
        return 1
      fi
    done
    return 0
  fi

  # hil-fleet.sh runs the two servers once for all its runners (the ports
  # are host-global); a runner then only checks they are up and stops
  # nothing (NET_LISTENER_PIDS stays empty).
  if [[ "${PICODROID_NET_LISTENERS_EXTERNAL:-0}" == "1" ]]; then
    local port
    for port in "$NET_ECHO_PORT" "$NET_HTTP_PORT"; do
      if ! net_wait_port "$port" 5; then
        NET_LISTENER_ERR="net listeners: PICODROID_NET_LISTENERS_EXTERNAL=1 but nothing listens on port $port"
        return 1
      fi
    done
    return 0
  fi

  local tool
  for tool in socat python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      NET_LISTENER_ERR="net listeners: '$tool' not installed"
      return 1
    fi
  done
  local port
  for port in "$NET_ECHO_PORT" "$NET_HTTP_PORT"; do
    if net_port_open "$port"; then
      NET_LISTENER_ERR="net listeners: port $port is already in use"
      return 1
    fi
  done

  local www
  www="$(mktemp -d)"
  # setsid: each server gets its own process group so stop_net_listeners can
  # kill the group (socat forks a child per connection) without touching us.
  setsid socat "TCP-LISTEN:${NET_ECHO_PORT},fork,reuseaddr" EXEC:cat \
    > "$log_dir/net-echo.log" 2>&1 < /dev/null &
  NET_LISTENER_PIDS+=($!)
  setsid python3 -m http.server "$NET_HTTP_PORT" --bind 0.0.0.0 --directory "$www" \
    > "$log_dir/net-http.log" 2>&1 < /dev/null &
  NET_LISTENER_PIDS+=($!)

  for port in "$NET_ECHO_PORT" "$NET_HTTP_PORT"; do
    if ! net_wait_port "$port" 5; then
      NET_LISTENER_ERR="net listeners: port $port did not come up (see $log_dir/net-*.log)"
      stop_net_listeners
      return 1
    fi
  done
  return 0
}

# Stop the servers started by start_net_listeners. Kills by PID only — never
# by name pattern: `pkill -f socat` would also match the shell that launched
# this script if its command line mentions the word.
stop_net_listeners() {
  local pid
  for pid in ${NET_LISTENER_PIDS[@]+"${NET_LISTENER_PIDS[@]}"}; do
    kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
  done
  for pid in ${NET_LISTENER_PIDS[@]+"${NET_LISTENER_PIDS[@]}"}; do
    wait "$pid" 2>/dev/null || true
  done
  NET_LISTENER_PIDS=()
}

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
    rp2350) PROBE_CHIP="RP235x" ;;
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

# The same lock, held by the calling shell across several commands (fd 9):
# build-apk.sh keeps it from gradlew through the copy of Gradle's output,
# because that output file is shared by every caller and two parallel
# builds of one app in different shrink modes would otherwise swap papks.
# Commands run while it is held must close fd 9 (`9>&-`) so a Gradle daemon
# cannot inherit and keep it. Same 600 s deadlock detector as above.
GRADLE_LOCK_HELD=0
gradle_lock_acquire() {
  mkdir -p "$REPO_ROOT/build"
  command -v flock >/dev/null 2>&1 || return 0
  exec 9>"$REPO_ROOT/build/.gradle.lock"
  flock -w 600 9 || { echo "gradle lock: could not take build/.gradle.lock within 600 s" >&2; return 1; }
  GRADLE_LOCK_HELD=1
}
gradle_lock_release() {
  [[ "$GRADLE_LOCK_HELD" == 1 ]] || return 0
  flock -u 9
  exec 9>&-
  GRADLE_LOCK_HELD=0
}

# The same lock, held by the calling shell across several commands (fd 9):
# build-apk.sh keeps it from gradlew through the copy of Gradle's output,
# because that output file is shared by every caller and two parallel
# builds of one app in different shrink modes would otherwise swap papks.
# Commands run while it is held must close fd 9 (`9>&-`) so a Gradle daemon
# cannot inherit and keep it. Same 600 s deadlock detector as above.
GRADLE_LOCK_HELD=0
gradle_lock_acquire() {
  mkdir -p "$REPO_ROOT/build"
  command -v flock >/dev/null 2>&1 || return 0
  exec 9>"$REPO_ROOT/build/.gradle.lock"
  flock -w 600 9 || { echo "gradle lock: could not take build/.gradle.lock within 600 s" >&2; return 1; }
  GRADLE_LOCK_HELD=1
}
gradle_lock_release() {
  [[ "$GRADLE_LOCK_HELD" == 1 ]] || return 0
  flock -u 9
  exec 9>&-
  GRADLE_LOCK_HELD=0
}

# Takes the lease on a dev board for the caller's session, or exits 75
# (EX_TEMPFAIL) with the holder and a hint.
#
# Several parallel sessions, one lease per board: every script that flashes,
# power-cycles or talks pdb to a board calls this first. If the board is
# free the lease is taken and kept -- it belongs to the *session* (inside
# Claude the claude process, in a terminal the shell that ran the script),
# not to this command, so a flash followed by pdb calls needs no ceremony and
# nothing can interleave. Release with `./scripts/device-lock.sh release`.
# The lease evaporates on its own when the owning process exits.
#
# Optional leading `--wait SECS` queues instead of failing. The remaining
# args label the lease in `status`; with a fleet config they are also
# scanned for the board (--slot NAME, --board/-b NAME, --boards A,B, or
# -s /dev/ttyACMn), falling back to the slot this session already holds or
# the only slot configured (fleet-lib.sh::fleet_resolve_slot). In fleet mode
# the slot's identity is exported for the caller: PICODROID_SLOT,
# PICODROID_PROBE_SERIAL, PICODROID_BOARD_USB_PATH and PROBE_RS_PROBE, so
# probe-rs talks to this slot's probe and nothing else.
#
# PICODROID_DEVICE_LOCK=0 skips the check (emergencies) but still resolves
# the slot. Without flock the check is skipped too, matching gradle_lock_run.
require_device_lock() {
  # $PPID in a sourced function is the parent of the script, i.e. the shell
  # (or Claude session) that launched it -- the lease must outlive the script.
  local owner_pid="${PICODROID_DEVICE_OWNER_PID:-${CLAUDE_PID:-$PPID}}"
  local -a slot_args=()
  if fleet_enabled; then
    local held slot
    held="$(PICODROID_DEVICE_OWNER_PID="$owner_pid" bash "$SCRIPT_DIR/device-lock.sh" mine 2>/dev/null || true)"
    slot=$(fleet_resolve_slot "$held" "$@") || exit 1
    fleet_export_slot "$slot"
    slot_args=(--slot "$slot")
  fi
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
  PICODROID_DEVICE_OWNER_PID="$owner_pid" \
    bash "$SCRIPT_DIR/device-lock.sh" acquire ${slot_args[@]+"${slot_args[@]}"} \
      ${wait_args[@]+"${wait_args[@]}"} \
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

# Builds the system apps a multi-app board links into its firmware (the
# launcher; docs/designs/multi-app-2026-09.md D11) and exports the two
# variables picodroid-core/build.rs reads: PICODROID_SYSTEM_APKS, a
# colon-separated list of their .papk paths, and PICODROID_BOOT (what
# `flash.sh --boot` set, if anything).
#
# Requires resolve_board (MAX_INSTALLED_APPS, BOARD). The PAPKs take the
# same shape as the app under test: --strip-debug, --keep-lines when the
# caller passes it (a firmware that keeps line numbers), --board for the
# contract check; the shrink flags ride the exported PICODROID_SHRINK*. A
# single-app board gets an empty list and embeds nothing.
#
# Both variables are exported even when empty. build.rs declares them
# rerun-if-env-changed, and flash.sh runs cargo twice (build, then run): a
# variable set for one call and unset for the other would rebuild the
# firmware without the launcher and flash that. PICODROID_PREBUILT_SYSTEM_APKS
# short-circuits the Gradle build the way PICODROID_PREBUILT_APK does for
# the app: pre-commit builds the launcher once in its serial prologue so
# parallel lanes never race on system-apps/launcher/build/.
build_system_apks() {
  local keep_lines=()
  [[ "${1:-}" == "--keep-lines" ]] && keep_lines=(--keep-lines)
  export PICODROID_BOOT="${PICODROID_BOOT:-}"
  if [[ "${MAX_INSTALLED_APPS:-1}" -le 1 ]]; then
    export PICODROID_SYSTEM_APKS=""
    return 0
  fi
  if [[ -n "${PICODROID_PREBUILT_SYSTEM_APKS:-}" ]]; then
    local prebuilt p
    IFS=':' read -ra prebuilt <<< "$PICODROID_PREBUILT_SYSTEM_APKS"
    for p in "${prebuilt[@]}"; do
      if [[ ! -f "$p" ]]; then
        echo "PICODROID_PREBUILT_SYSTEM_APKS does not exist: $p" >&2
        return 1
      fi
    done
    export PICODROID_SYSTEM_APKS="$PICODROID_PREBUILT_SYSTEM_APKS"
    return 0
  fi
  local list="" dir name
  for dir in "$REPO_ROOT"/system-apps/*/; do
    [[ -f "$dir/PicodroidManifest.xml" ]] || continue
    name="$(basename "$dir")"
    bash "$SCRIPT_DIR/build-apk.sh" --app "$name" --strip-debug \
      ${keep_lines[@]+"${keep_lines[@]}"} ${BOARD:+--board "$BOARD"} || return 1
    list="${list:+$list:}$REPO_ROOT/build/apks/${name}.papk"
  done
  export PICODROID_SYSTEM_APKS="$list"
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

  # Step 1b: the system apps this board's firmware carries (multi-app M2).
  build_system_apks ${keep_lines[@]+"${keep_lines[@]}"} || return 1

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
  #
  # FIRMWARE_PROFILE_ARGS is an output, like FIRMWARE_FEATURES: profile keys
  # are part of cargo's fingerprint, so a second cargo invocation on the same
  # package without them (flash.sh's `cargo run`) rebuilds the whole tree
  # under the stock profile and flashes that larger image instead of the one
  # measured here. Every cargo call on the firmware passes both arrays.
  FIRMWARE_PROFILE_ARGS=(
    --config 'profile.dev.debug-assertions=false'
    --config 'profile.dev.overflow-checks=false'
  )
  if [[ "$TARGET" == thumbv6m* ]]; then
    FIRMWARE_PROFILE_ARGS+=(--config 'profile.release.lto=false')
  fi
  # `return`, not a bare command: a caller that invokes build_firmware on the
  # left of `||` runs it with errexit disabled, so a failed cargo used to fall
  # straight through to the ELF check below -- measuring or flashing whatever
  # stale binary the last good build left behind. Report the failure instead.
  # shellcheck disable=SC2086  # CARGO_PLUS is intentionally unquoted (empty or a "+toolchain" override)
  if ! PICODROID_APK_PATH="$APK_PATH" cargo $CARGO_PLUS build \
    --manifest-path "$MANIFEST_DIR/Cargo.toml" \
    "${FIRMWARE_PROFILE_ARGS[@]}" \
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
