#!/usr/bin/env bash
set -e

CHIP="rp2040"
APP=""
PROFILE="debug"
UF2=false
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --chip)
      CHIP="$2"
      shift 2
      ;;
    --app)
      APP="$2"
      shift 2
      ;;
    --release)
      PROFILE="release"
      EXTRA_ARGS+=("--release")
      shift
      ;;
    --uf2)
      UF2=true
      shift
      ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

case "$CHIP" in
  rp2040)
    TARGET="thumbv6m-none-eabi"
    CHIP_FEATURE="chip-rp2040"
    FLASH_MAX=2097152   # 2 MB
    RAM_MAX=270336      # 264 KB
    UF2_FAMILY="0xe48bff56"
    ;;
  rp2350)
    TARGET="thumbv8m.main-none-eabihf"
    CHIP_FEATURE="chip-rp2350"
    FLASH_MAX=4194304   # 4 MB
    RAM_MAX=532480      # 520 KB
    UF2_FAMILY="0xe48bff59"
    ;;
  *)
    echo "Unknown chip: $CHIP. Use rp2040 or rp2350." >&2
    exit 1
    ;;
esac

APP_FEATURE="${APP:-blinky}"

cargo build \
  --target "$TARGET" \
  --no-default-features \
  --features "${APP_FEATURE},${CHIP_FEATURE}" \
  "${EXTRA_ARGS[@]}"

ELF="target/${TARGET}/${PROFILE}/picodroid"

if [[ ! -f "$ELF" ]]; then
  echo "Binary not found: $ELF" >&2
  exit 1
fi

SIZE_OUTPUT=$(arm-none-eabi-size "$ELF")
echo ""
echo "=== Memory Usage ==="
echo "$SIZE_OUTPUT"

read TEXT DATA BSS <<< $(echo "$SIZE_OUTPUT" | awk 'NR==2 {print $1, $2, $3}')
FLASH=$(( TEXT + DATA ))
RAM=$(( DATA + BSS ))

FLASH_PCT=$(( FLASH * 100 / FLASH_MAX ))
RAM_PCT=$(( RAM * 100 / RAM_MAX ))

printf "  Flash: %d / %d bytes (%d%%)\n" "$FLASH" "$FLASH_MAX" "$FLASH_PCT"
printf "  RAM:   %d / %d bytes (%d%%)\n" "$RAM" "$RAM_MAX" "$RAM_PCT"
echo ""

if [[ "$UF2" == true ]]; then
  UF2_OUT="${ELF}.uf2"
  if ! command -v elf2uf2-rs &>/dev/null; then
    echo "elf2uf2-rs not found. Install with: cargo install elf2uf2-rs" >&2
    exit 1
  fi
  elf2uf2-rs "$ELF" "$UF2_OUT"
  echo "UF2 written to: $UF2_OUT"
fi
