#!/usr/bin/env bash
# Shared helpers sourced by build.sh and flash.sh

# Sets TARGET, CHIP_FEATURE, FLASH_MAX, RAM_MAX, UF2_FAMILY based on $CHIP.
resolve_chip() {
  local chip="$1"
  case "$chip" in
    rp2040)
      TARGET="thumbv6m-none-eabi"
      CHIP_FEATURE="chip-rp2040"
      PROBE_CHIP="RP2040"
      PAPK_META_ADDR="0x10100000"  # XIP base + PAPK_FLASH_META_OFFSET
      FLASH_MAX=2097152   # 2 MB
      RAM_MAX=270336      # 264 KB
      UF2_FAMILY="0xe48bff56"
      ;;
    rp2350)
      TARGET="thumbv8m.main-none-eabihf"
      CHIP_FEATURE="chip-rp2350"
      PROBE_CHIP="RP2350"
      PAPK_META_ADDR="0x10300000"  # XIP base + PAPK_FLASH_META_OFFSET
      FLASH_MAX=4194304   # 4 MB
      RAM_MAX=532480      # 520 KB
      UF2_FAMILY="0xe48bff59"
      ;;
    *)
      echo "Unknown chip: $chip. Use rp2040 or rp2350." >&2
      exit 1
      ;;
  esac
}

# Returns the number of logical CPUs (cross-platform: Linux + macOS).
cpu_count() {
  nproc 2>/dev/null || sysctl -n hw.logicalcpu
}

# Prints available app names from the examples directory, one per line, indented.
list_apps() {
  local examples_dir="$1"
  for d in "$examples_dir"/*/; do
    [[ -d "$d" ]] && echo "    $(basename "$d")"
  done
}

# Prints flash/RAM usage for a given ELF. Requires FLASH_MAX and RAM_MAX to be set.
print_memory_usage() {
  local elf="$1"
  local size_output
  size_output=$(arm-none-eabi-size "$elf")
  echo ""
  echo "=== Memory Usage ==="
  echo "$size_output"

  read TEXT DATA BSS <<< $(echo "$size_output" | awk 'NR==2 {print $1, $2, $3}')
  local flash=$(( TEXT + DATA ))
  local ram=$(( DATA + BSS ))

  printf "  Flash: %d / %d bytes (%d%%)\n" "$flash" "$FLASH_MAX" "$(( flash * 100 / FLASH_MAX ))"
  printf "  RAM:   %d / %d bytes (%d%%)\n" "$ram" "$RAM_MAX" "$(( ram * 100 / RAM_MAX ))"
  echo ""
}
