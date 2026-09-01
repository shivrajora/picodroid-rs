#!/usr/bin/env bash
# Regenerate sdk/api-contract.tsv — the java/** surface pico-jvm serves —
# from the runtime's own tables, then prove the fresh copy passes the
# currency test.
#
# The generator is picodroid-core's `api_contract_is_current` test
# (picodroid-core/src/native_handler/api_contract.rs); with
# PICODROID_UPDATE_API_CONTRACT=1 it rewrites the file instead of failing.
# It needs FRAMEWORK_CLASSES embedded, which needs PICODROID_APK_PATH at
# build.rs time — the same helloworld bootstrap scripts/test.sh uses.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

HOST_TARGET="$(host_target)"
CONTRACT="$REPO_ROOT/sdk/api-contract.tsv"

APK_PATH="$REPO_ROOT/build/apks/helloworld.papk"
if [[ ! -f "$APK_PATH" ]]; then
  echo "==> Building helloworld APK (required for framework-class embedding)..."
  bash "$REPO_ROOT/scripts/build-apk.sh" --app helloworld
fi

echo "==> Regenerating sdk/api-contract.tsv..."
PICODROID_APK_PATH="$APK_PATH" PICODROID_UPDATE_API_CONTRACT=1 \
  cargo test -p picodroid-core --target "$HOST_TARGET" api_contract_is_current -- --nocapture 2>&1 \
  | grep -E "^wrote |^test result|panicked|error" || true

echo "==> Verifying the regenerated file passes the currency test..."
PICODROID_APK_PATH="$APK_PATH" \
  cargo test -p picodroid-core --target "$HOST_TARGET" api_contract 2>&1 | grep -E "^test |^test result"

echo "==> Change against the committed copy:"
(cd "$REPO_ROOT" && git --no-pager diff --stat -- sdk/api-contract.tsv || true)
