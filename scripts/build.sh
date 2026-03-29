#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

CHIP="rp2040"
APP="helloworld"
PROFILE="debug"
UF2=false
EXTRA_ARGS=()

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -c, --chip <chip>   Target chip: rp2040 (default) or rp2350
  -a, --app  <app>    App to build and install: helloworld (default), blinky, uart, etc.
  -r, --release       Build in release mode (default: debug)
  -u, --uf2           Convert output ELF to UF2 (requires elf2uf2-rs)
  -h, --help          Show this help message

Examples:
  $(basename "$0")
  $(basename "$0") -c rp2350 -a helloworld -r -u
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    -c|--chip)
      CHIP="$2"
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
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

resolve_chip "$CHIP"

# Step 1: Build the APK for the selected app.
bash "$SCRIPT_DIR/build-apk.sh" --app "$APP"

APK_PATH="$SCRIPT_DIR/../build/apks/${APP}.papk"

# Step 2: Build the firmware, embedding the APK.
JOBS=$(cpu_count)
PICODROID_APK_PATH="$APK_PATH" cargo build \
  --jobs "$JOBS" \
  --target "$TARGET" \
  --no-default-features \
  --features "$CHIP_FEATURE" \
  "${EXTRA_ARGS[@]}"

ELF="target/${TARGET}/${PROFILE}/picodroid"

if [[ ! -f "$ELF" ]]; then
  echo "Binary not found: $ELF" >&2
  exit 1
fi

print_memory_usage "$ELF"

if [[ "$UF2" == true ]]; then
  UF2_OUT="${ELF}.uf2"
  if ! command -v elf2uf2-rs &>/dev/null; then
    echo "elf2uf2-rs not found. Install with: cargo install elf2uf2-rs" >&2
    exit 1
  fi
  elf2uf2-rs "$ELF" "$UF2_OUT"
  echo "UF2 written to: $UF2_OUT"
fi
