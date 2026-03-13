#!/usr/bin/env bash
set -e

CARGO_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)
      CARGO_ARGS+=("--no-default-features" "--features" "$2")
      shift 2
      ;;
    *)
      CARGO_ARGS+=("$1")
      shift
      ;;
  esac
done

cargo run "${CARGO_ARGS[@]}"
