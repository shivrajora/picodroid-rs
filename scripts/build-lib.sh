#!/usr/bin/env bash
# Build drivers: the system APKs, the firmware, and the memory-usage report.
#
# Sourced by lib.sh (which sets REPO_ROOT and SCRIPT_DIR); do not run or
# source directly. Functions only -- no work happens at source time.

# Bytes an ELF links into the app region (PAPK_FLASH) rather than the program
# region: the `.papk_flash_init` section build_support/papk.rs emits (the
# boot-meta sector plus the embedded PAPK). 0 for an image without one.
app_region_bytes() {
  "$SIZE_TOOL" -A "$1" | awk '$1 == ".papk_flash_init" { n += $2 } END { print n + 0 }'
}

# Exports TEXT/DATA/BSS (size(1) as printed), APP_REGION_BYTES and FLASH_BYTES,
# the program-region figure, for callers that log them (parity-bench.sh).
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
  # size(1) folds every allocated read-only section into `text`, the embedded
  # PAPK included, but the linker places that one in PAPK_FLASH: counted here
  # it charged the app's bytes to the program region, the budget this line
  # gates (docs/designs/flash-budget-2026-09.md §3). Leave it out, and report
  # it against its own region.
  APP_REGION_BYTES=$(app_region_bytes "$elf")
  FLASH_BYTES=$(( TEXT + DATA - APP_REGION_BYTES ))
  local flash=$FLASH_BYTES
  local ram=$(( DATA + BSS ))

  printf "  Flash: %d / %d bytes (%d%% of program region; chip total %d)\n" \
    "$flash" "$PROGRAM_FLASH_MAX" "$(( flash * 100 / PROGRAM_FLASH_MAX ))" "$FLASH_MAX"
  if (( APP_REGION_BYTES > 0 )); then
    printf "  App region: %d / %d bytes (embedded PAPK + boot-meta sector; not in Flash:)\n" \
      "$APP_REGION_BYTES" "$(( APP_REGION_KB * 1024 ))"
  fi
  printf "  RAM:   %d / %d bytes (%d%%)\n" "$ram" "$RAM_MAX" "$(( ram * 100 / RAM_MAX ))"
  # What .data + .bss leave of RAM is the core-0 main stack: the boot path,
  # then every core-0 interrupt for the life of the firmware (flip-link puts
  # it below .bss, so an overflow runs off the start of RAM and the core
  # locks up before a single log line). Static growth erodes it silently —
  # the network boards were down to 4.4 KB when their debug image stopped
  # booting (2026-09-04) — so a build that leaves less than the floor fails
  # here, in every script that builds firmware, instead of on the board.
  #
  # Read it off the link, not off size(1): `.data` here is executable (the
  # RAM-resident flash routines), so Berkeley size files it under text and
  # RAM_MAX - (data + bss) overstated the stack by ~10 KB on the touch kit —
  # 11,816 B reported, 2,048 B linked, and the boot-time LittleFS sweep
  # overflowed it (HIL nightly 2026-09-17). flip-link places the stack at
  # the bottom of RAM and defines `_stack_end`/`_stack_start` as its bounds,
  # so their difference is the whole stack. The linker script itself asserts
  # the same floor (build_support/flash_layout.rs `__main_stack_floor`);
  # this is the report.
  local nm_tool="${SIZE_TOOL%size}nm"
  local stack_start stack_end headroom
  if command -v "$nm_tool" &>/dev/null; then
    stack_start=$("$nm_tool" "$elf" | awk '$3 == "_stack_start" { print $1 }')
    stack_end=$("$nm_tool" "$elf" | awk '$3 == "_stack_end" { print $1 }')
  fi
  if [[ -n "$stack_start" && -n "$stack_end" ]]; then
    headroom=$(( 0x$stack_start - 0x$stack_end ))
  else
    headroom=$(( RAM_MAX - ram ))
    echo "  (no $nm_tool: main-stack headroom estimated from size(1), which omits executable .data)"
  fi
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
