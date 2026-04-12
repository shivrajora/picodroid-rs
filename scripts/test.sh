#!/usr/bin/env bash
# Run unit tests on the host target (required for no_std crate; default target is thumbv6m-none-eabi)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

cargo test --workspace --jobs "$(cpu_count)" --target "$(host_target)"
