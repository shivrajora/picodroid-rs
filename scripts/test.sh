#!/usr/bin/env bash
# Run unit tests on the host target (required for no_std crate; default target
# is thumbv6m-none-eabi).
#
# Runs the workspace twice — once with `PICODROID_SHRINK=0` (identity shrink
# map) and once with `PICODROID_SHRINK=1` (active map). This exercises
# `src/dispatch_sites.rs::every_site_resolves_under_active_shrink_map` under
# both maps, the regression guard for the shrink-breaks-callbacks bug fixed
# in eba57c3.
#
# `--arch arm32` adds a third leg on 32-bit ARM under qemu. It is opt-in and
# off by default, so pre-commit never needs an emulator. What it buys: the
# host is 64-bit and the devices are not, so any test whose subject is
# pointer width or `usize` arithmetic has been passing here for reasons that
# do not hold on hardware — papk-format's offset overflow checks and the
# LVGL handle table are the two known cases.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

JOBS="$(cpu_count)"
HOST_TARGET="$(host_target)"
TEST_ARCH="${PICODROID_TEST_ARCH:-host}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) TEST_ARCH="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $(basename "$0") [--arch <host|arm32>]"
      echo
      echo "  host   (default) native target, both shrink modes"
      echo "  arm32  adds a 32-bit ARM leg under qemu-user; needs"
      echo "         qemu-user-static + gcc-arm-linux-gnueabihf"
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

case "$TEST_ARCH" in
  host|arm32) ;;
  *) echo "Unknown --arch: '$TEST_ARCH' (expected host or arm32)" >&2; exit 1 ;;
esac

# The dispatch_sites test needs FRAMEWORK_CLASSES embedded, which requires
# PICODROID_APK_PATH at build.rs time. Build helloworld once and reuse it
# for both shrink modes.
APK_PATH="$REPO_ROOT/build/apks/helloworld.papk"
if [[ ! -f "$APK_PATH" ]]; then
  echo "==> Building helloworld APK (required for framework-class embedding)..."
  bash "$REPO_ROOT/scripts/build-apk.sh" --app helloworld
fi

echo "==> Running tests (no-shrink)..."
PICODROID_APK_PATH="$APK_PATH" \
  cargo test --workspace --jobs "$JOBS" --target "$HOST_TARGET"

# Re-build the APK under the active shrink map so the embedded framework
# classes match what PICODROID_SHRINK=1 expects.
echo "==> Building helloworld APK (shrink)..."
bash "$REPO_ROOT/scripts/build-apk.sh" --app helloworld --shrink

echo "==> Running tests (shrink)..."
PICODROID_APK_PATH="$APK_PATH" PICODROID_SHRINK=1 \
  cargo test --workspace --jobs "$JOBS" --target "$HOST_TARGET"

if [[ "$TEST_ARCH" == "arm32" ]]; then
  if ! have_arm32_sim; then
    echo "==> --arch arm32 requested but the toolchain is incomplete." >&2
    arm32_sim_hint
    exit 1
  fi
  ARM32_TARGET="$(sim_target arm32)"

  # Crates are named explicitly rather than using --workspace: tools/pdb
  # depends on serialport -> libudev-sys, which needs an armhf pkg-config
  # sysroot we do not have and do not want. Everything listed below is
  # portable Rust plus the C we already cross-compile.
  ARM32_CRATES=(-p pico-jvm -p papk-format -p compat -p pdb-protocol -p picodroid-core -p picodroid)

  # One shrink mode only. Shrink-map invariance is what the two host legs
  # above are for; this leg is here for pointer width and ARM codegen, which
  # are orthogonal to which class-name map is active.
  echo "==> Rebuilding helloworld APK (no-shrink) for the arm32 leg..."
  bash "$REPO_ROOT/scripts/build-apk.sh" --app helloworld

  echo "==> Running tests (arm32 / qemu)..."
  PICODROID_APK_PATH="$APK_PATH" \
    cargo test "${ARM32_CRATES[@]}" --jobs "$JOBS" --target "$ARM32_TARGET"

  # At 32 bits the generational handle table is behind handle-table-32 (it is
  # the staged replacement for the raw-pointer cast the devices ship today).
  # Without this leg its 32-bit arm is only ever compiled, never run.
  echo "==> Running tests (arm32 / qemu, handle-table-32)..."
  PICODROID_APK_PATH="$APK_PATH" \
    cargo test -p picodroid-core --features handle-table-32 \
      --jobs "$JOBS" --target "$ARM32_TARGET"
fi
