#!/usr/bin/env bash
# Regenerate sdk/api-contract.tsv — the java/** surface pico-jvm serves —
# from the runtime's own tables, sdk/class-names.tsv — every class the
# runtime may name (the source of the `c::` consts) — and
# sdk/member-names.tsv — every method and field the SDK declares (the
# source of the `m::` consts) — then prove the fresh copies pass their
# currency tests.
#
# The generators are picodroid-core's `api_contract_is_current` and
# `member_names_are_current` tests (picodroid-core/src/native_handler/
# {api_contract,member_names}.rs); with PICODROID_UPDATE_API_CONTRACT=1 /
# PICODROID_UPDATE_MEMBER_NAMES=1 they rewrite the file instead of failing.
# They need FRAMEWORK_CLASSES embedded, which needs PICODROID_APK_PATH at
# build.rs time — the same helloworld bootstrap scripts/test.sh uses.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

HOST_TARGET="$(host_target)"
CONTRACT="$REPO_ROOT/sdk/api-contract.tsv"
MEMBER_NAMES="$REPO_ROOT/sdk/member-names.tsv"

APK_PATH="$REPO_ROOT/build/apks/helloworld.papk"
if [[ ! -f "$APK_PATH" ]]; then
  echo "==> Building helloworld APK (required for framework-class embedding)..."
  bash "$REPO_ROOT/scripts/build-apk.sh" --app helloworld
fi

echo "==> Regenerating sdk/api-contract.tsv..."
PICODROID_APK_PATH="$APK_PATH" PICODROID_UPDATE_API_CONTRACT=1 \
  cargo test -p picodroid-core --target "$HOST_TARGET" api_contract_is_current -- --nocapture 2>&1 \
  | grep -E "^wrote |^test result|panicked|error" || true

echo "==> Regenerating sdk/class-names.tsv..."
PICODROID_APK_PATH="$APK_PATH" PICODROID_UPDATE_CLASS_NAMES=1 \
  cargo test -p picodroid-core --target "$HOST_TARGET" class_names_are_current -- --nocapture 2>&1 \
  | grep -E "^wrote |^test result|panicked|error" || true

echo "==> Regenerating sdk/member-names.tsv..."
PICODROID_APK_PATH="$APK_PATH" PICODROID_UPDATE_MEMBER_NAMES=1 \
  cargo test -p picodroid-core --target "$HOST_TARGET" member_names_are_current -- --nocapture 2>&1 \
  | grep -E "^wrote |^test result|panicked|error" || true

echo "==> Verifying the regenerated files pass their currency tests..."
# A fresh member-names.tsv is a build.rs input (the m:: consts), so this run
# also rebuilds against it.
PICODROID_APK_PATH="$APK_PATH" \
  cargo test -p picodroid-core --target "$HOST_TARGET" -- api_contract member_names class_names 2>&1 \
  | grep -E "^test |^test result"

echo "==> Change against the committed copies:"
(cd "$REPO_ROOT" && git --no-pager diff --stat -- sdk/api-contract.tsv sdk/member-names.tsv sdk/class-names.tsv || true)
