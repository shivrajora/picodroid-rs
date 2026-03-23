#!/usr/bin/env bash
# Run unit tests on the host target (required for no_std crate; default target is thumbv6m-none-eabi)
set -euo pipefail

JOBS=$(nproc 2>/dev/null || sysctl -n hw.logicalcpu)
cargo test --jobs "$JOBS" --target "$(rustc -vV | awk '/^host:/ { print $2 }')"
