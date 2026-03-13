#!/usr/bin/env bash
set -e

cargo build "$@"

PROFILE="debug"
for arg in "$@"; do
  [[ "$arg" == "--release" ]] && PROFILE="release"
done

ELF="target/thumbv6m-none-eabi/${PROFILE}/picodroid"

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

FLASH_MAX=2097152   # 2MB flash
RAM_MAX=270336      # 264KB RAM

FLASH_PCT=$(( FLASH * 100 / FLASH_MAX ))
RAM_PCT=$(( RAM * 100 / RAM_MAX ))

printf "  Flash: %d / %d bytes (%d%%)\n" "$FLASH" "$FLASH_MAX" "$FLASH_PCT"
printf "  RAM:   %d / %d bytes (%d%%)\n" "$RAM" "$RAM_MAX" "$RAM_PCT"
echo ""
