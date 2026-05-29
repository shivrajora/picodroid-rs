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

# platforms/esp is its own Cargo workspace (separate Cargo.lock to keep the
# Xtensa-only deps out of the main resolver), so it isn't reached by the
# --workspace runs above. Run its host-target tests explicitly.
echo "==> Running tests (platforms/esp)..."
PICODROID_APK_PATH="$APK_PATH" \
  cargo test --manifest-path "$REPO_ROOT/platforms/esp/Cargo.toml" \
    --jobs "$JOBS" --target "$HOST_TARGET"
