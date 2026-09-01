#!/usr/bin/env bash
# Run unit tests on the host target (required for no_std crate; default target
# is thumbv6m-none-eabi).
#
# Runs the workspace twice — once with `PICODROID_SHRINK=0` (identity shrink
# map) and once with `PICODROID_SHRINK=1` (active map). This exercises
# `src/dispatch_sites.rs::every_site_resolves_under_active_shrink_map` under
# both maps, the regression guard for the shrink-breaks-callbacks bug fixed
# in eba57c3.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

JOBS="$(cpu_count)"
HOST_TARGET="$(host_target)"

# The workspace profiles are tuned for the *firmware* image: [profile.test] in
# Cargo.toml inherits codegen-units = 1, opt-level = 3, incremental = false.
# Nothing about the host test binary is flash- or ratchet-constrained, and this
# script compiles it twice (once per shrink mode), so the single-CU
# non-incremental codegen is paid twice over ~1159 tests for no benefit.
#
# Override it here rather than in Cargo.toml: [profile.dev] and [profile.release]
# feed flashed images -- release is what bench/parity/ratchet.toml is baselined
# against, dev is what the 896K debug flash gate measures -- so a profile edit
# there would move the image. --config on a host-target `cargo test` cannot
# reach either.
#
# PICODROID_TEST_FAST=0 reproduces the stock profile exactly.
TEST_PROFILE_ARGS=()
if [[ "${PICODROID_TEST_FAST:-1}" == "1" ]]; then
  TEST_PROFILE_ARGS=(
    --config profile.test.codegen-units=16
    --config profile.test.incremental=true
    --config profile.test.opt-level=2
  )
fi

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
  cargo test --workspace --jobs "$JOBS" --target "$HOST_TARGET" \
  "${TEST_PROFILE_ARGS[@]}"

# Re-build the APK under the active shrink map so the embedded framework
# classes match what PICODROID_SHRINK=1 expects.
#
# To its own path, NOT over build/apks/helloworld.papk. That file is what the
# firmware flash gate and the size ratchet link and measure, and this script
# used to swap a shrunk papk in underneath them halfway through -- invisible
# while everything was serial, a live race once pre-commit runs lanes in
# parallel.
SHRINK_APK_PATH="$REPO_ROOT/build/apks/shrink/helloworld.papk"
echo "==> Building helloworld APK (shrink)..."
bash "$REPO_ROOT/scripts/build-apk.sh" --app helloworld --shrink -o "$SHRINK_APK_PATH"

echo "==> Running tests (shrink)..."
PICODROID_APK_PATH="$SHRINK_APK_PATH" PICODROID_SHRINK=1 \
  cargo test --workspace --jobs "$JOBS" --target "$HOST_TARGET" \
  "${TEST_PROFILE_ARGS[@]}"
