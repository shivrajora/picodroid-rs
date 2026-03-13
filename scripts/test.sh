#!/usr/bin/env bash
# Run unit tests on the host target (required for no_std crate; default target is thumbv6m-none-eabi)
set -euo pipefail

cargo test --target "$(rustc -vV | awk '/^host:/ { print $2 }')"
