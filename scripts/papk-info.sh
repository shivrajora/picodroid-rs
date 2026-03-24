#!/usr/bin/env bash
# Display the contents of a .papk file.
#
# Usage: ./scripts/papk-info.sh <file.papk>
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [[ $# -eq 0 || "$1" == "-h" || "$1" == "--help" ]]; then
  cat <<EOF
Usage: $(basename "$0") <file.papk>

Displays a structured summary of a PAPK file, including:
  - File header (magic, version, section offsets)
  - Manifest entries (main-class, package-name, version)
  - Class table (JVM name and bytecode size for each class)

Example:
  ./scripts/build-apk.sh --app helloworld
  ./scripts/papk-info.sh build/apks/helloworld.papk
EOF
  exit 0
fi

HOST_TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"

cargo run \
  --quiet \
  --target "$HOST_TARGET" \
  --manifest-path "$SCRIPT_DIR/../tools/papk-info/Cargo.toml" \
  -- "$@"
