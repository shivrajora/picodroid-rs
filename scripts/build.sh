#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

BOARD="testbench_rp2350"
APP="helloworld"
PROFILE="debug"
UF2=false
EXTRA_ARGS=()

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -b, --board <board>  Target board (default: testbench_rp2350)
  -a, --app  <app>     App to build and install (default: helloworld)
  -r, --release        Build in release mode (default: debug)
  -u, --uf2            Convert output ELF to UF2 (requires elf2uf2-rs)
      --shrink         Apply the active release class-name shrink map
                       (off by default; see docs/shrinker.md)
  -h, --help           Show this help message

Boards:
$(list_boards)

Apps:
$(list_apps "$SCRIPT_DIR/../examples")

Examples:
  $(basename "$0")
  $(basename "$0") -b testbench_rp2350 -a helloworld -r -u
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    -b|--board)
      BOARD="$2"
      shift 2
      ;;
    -a|--app)
      APP="$2"
      shift 2
      ;;
    -r|--release)
      PROFILE="release"
      EXTRA_ARGS+=("--release")
      shift
      ;;
    -u|--uf2)
      UF2=true
      shift
      ;;
    --shrink)
      export PICODROID_SHRINK=1
      shift
      ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

resolve_board "$BOARD"
build_firmware

if [[ "$UF2" == true ]]; then
  if [[ "$PLATFORM" == "esp" ]]; then
    echo "UF2 is not supported for ESP targets. Use espflash directly." >&2
    exit 1
  fi
  UF2_OUT="${ELF}.uf2"
  # Check if this is an RP2350-based board (target contains thumbv8m)
  if [[ "$TARGET" == *"thumbv8m"* ]]; then
    if ! command -v picotool &>/dev/null; then
      echo "picotool not found. Install with: brew install picotool" >&2
      exit 1
    fi
    picotool uf2 convert "$ELF" -t elf "$UF2_OUT" --family rp2350-arm-s
  else
    if ! command -v elf2uf2-rs &>/dev/null; then
      echo "elf2uf2-rs not found. Install with: cargo install elf2uf2-rs" >&2
      exit 1
    fi
    elf2uf2-rs "$ELF" "$UF2_OUT"
  fi
  echo "UF2 written to: $UF2_OUT"
fi
