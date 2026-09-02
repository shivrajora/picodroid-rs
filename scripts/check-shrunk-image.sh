#!/usr/bin/env bash
# Prove a --shrink firmware image carries no original Java name.
#
# Usage: ./scripts/check-shrunk-image.sh <elf> [<papk>]
#
# Scans every allocated section of the ELF (so .rodata, .text, .data and the
# embedded PAPK in .papk_flash_init) for the spellings unconditional shrinking
# is supposed to have removed: `java/`, `javax/` and `picodroid/` class
# names, descriptors naming one, and the most common contract member names
# in a dispatch-looking position. Any hit is a leak — a literal that dodged
# the `c::` / `m::` / `d::` consts (build_support/names.rs) — and fails the
# check with the offending strings listed. Runs in `pre-commit --full`
# against the rp2350 --release --shrink helloworld image.
set -euo pipefail

ELF="${1:?usage: check-shrunk-image.sh <elf>}"
OBJCOPY="${OBJCOPY:-arm-none-eabi-objcopy}"
if ! command -v "$OBJCOPY" >/dev/null 2>&1; then
  OBJCOPY=llvm-objcopy
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Flatten the allocated sections into one binary blob. --only-section would
# need the names; -O binary of the whole file writes every allocated section
# in address order, padding gaps with zeros, which is exactly the flash image.
"$OBJCOPY" -O binary "$ELF" "$tmp/image.bin"

# Class-name and descriptor spellings. `strings -n 6` keeps the scan cheap;
# every original name is longer than that.
leaks="$tmp/leaks.txt"
strings -n 6 "$tmp/image.bin" \
  | grep -oE '(L?(java|javax|picodroid)/[A-Za-z0-9_$/]+;?)' \
  | sort -u > "$leaks" || true

# Allow-list: text that legitimately mentions a package path without naming
# a class the shrinker maps — log/help prose, the API-hint advice strings.
# Keep this list short; every entry is a spelling the image ships on purpose.
allow='^(picodroid/net/|picodroid/pio/)$'
grep -vE "$allow" "$leaks" > "$tmp/real.txt" || true

if [[ -s "$tmp/real.txt" ]]; then
  echo "ERROR: --shrink image $ELF still spells original Java names:" >&2
  sed 's/^/    /' "$tmp/real.txt" >&2
  echo "Route each through the generated consts (c::/d::/m::, build_support/names.rs)." >&2
  exit 1
fi

# Member names: the served contract set in `.name(`-style positions cannot be
# distinguished from ordinary prose in a raw dump, so check the handful the
# runtime dispatches most and that no log message spells: a shrunk image
# has no business containing these as standalone strings.
members='^(toString|hashCode|equals|compareTo|charAt|substring|hasNext|iterator|getMessage|onCreate|setText|nativeCreate|fireClick|dispatchRunnable)$'
if strings -n 4 "$tmp/image.bin" | grep -qE "$members"; then
  echo "ERROR: --shrink image $ELF still spells served member names:" >&2
  strings -n 4 "$tmp/image.bin" | grep -E "$members" | sort -u | sed 's/^/    /' >&2
  exit 1
fi

echo "OK: $ELF carries no original Java class, descriptor or served member name."
