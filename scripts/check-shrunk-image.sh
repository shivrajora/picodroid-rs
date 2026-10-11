#!/usr/bin/env bash
# Prove a --shrink firmware image carries no original Java name.
#
# Usage: ./scripts/check-shrunk-image.sh <elf> [<app-package-prefix>]
#
# Scans every allocated section of the ELF (so .rodata, .text, .data and the
# embedded PAPK in .papk_flash_init) for the spellings unconditional shrinking
# is supposed to have removed: `java/`, `javax/` and `picodroid/` class
# names, descriptors naming one, and the most common contract member names
# in a dispatch-looking position. Any hit is a leak — a literal that dodged
# the `c::` / `m::` / `d::` consts (build_support/names.rs) — and fails the
# check with the offending strings listed. Runs in `pre-commit --full`
# against the rp2350 --release --shrink --shrink-app helloworld image.
#
# With an app package prefix (e.g. `helloworld/`), also proves --shrink-app
# renamed the app's own classes: the embedded PAPK must not spell the
# prefix anywhere but a string literal, and helloworld has none.
#
# A class the runtime names that no committed map covers yet is not a leak:
# a name added between releases keeps its original spelling until the next
# cut folds it in (website reference/shrinker.md, the append-only rule), in
# the firmware's `c::` const and in every PAPK alike. Such names are listed
# in sdk/class-names.tsv; the check reports them and goes on.
set -euo pipefail

ELF="${1:?usage: check-shrunk-image.sh <elf> [<app-package-prefix>]}"
APP_PREFIX="${2:-}"
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
# Dumped to files once and grepped from there: `strings | grep -q` under
# pipefail reports SIGPIPE (141) when grep matches early and closes the pipe,
# which reads as "no match" and passed a leaking image for weeks.
strings -n 4 "$tmp/image.bin" > "$tmp/strings4.txt"
strings -n 6 "$tmp/image.bin" > "$tmp/strings6.txt"
leaks="$tmp/leaks.txt"
grep -oE '(L?(java|javax|picodroid)/[A-Za-z0-9_$/]+;?)' "$tmp/strings6.txt" \
  | sort -u > "$leaks" || true

# Allow-list: text that legitimately mentions a package path without naming
# a class the shrinker maps — log/help prose, the API-hint advice strings.
# Keep this list short; every entry is a spelling the image ships on purpose.
allow='^(picodroid/net/|picodroid/pio/)$'
grep -vE "$allow" "$leaks" > "$tmp/candidates.txt" || true

# Names no committed map covers: in sdk/class-names.tsv but in no
# `from =` row. The maps are append-only supersets, so the union over the
# directory is the newest map's class set.
SDK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../sdk" && pwd)"
grep -h '^from *= *"' "$SDK_DIR"/shrink-maps/*.toml \
  | sed -E 's/^from *= *"([^"]+)".*/\1/' | sort -u > "$tmp/mapped.txt"
grep -vE '^(#|$)' "$SDK_DIR/class-names.tsv" | cut -f1 | sort -u \
  | comm -23 - "$tmp/mapped.txt" > "$tmp/unmapped.txt"

# `strings` runs adjacent literals together, so a candidate is the name
# with whatever followed it in flash. It is tolerated when it is an unmapped
# name followed by nothing that spells another package path; any candidate
# containing a mapped name is a leak.
: > "$tmp/real.txt"
: > "$tmp/pending.txt"
while IFS= read -r cand; do
  [[ -n "$cand" ]] || continue
  bare="${cand#L}"
  bare="${bare%;}"
  tolerated=
  if ! grep -qF -f "$tmp/mapped.txt" <<< "$bare"; then
    while IFS= read -r u; do
      [[ -n "$u" && "$bare" == "$u"* ]] || continue
      rest="${bare#"$u"}"
      if ! grep -qE '(java|javax|picodroid)/' <<< "$rest"; then
        tolerated=$u
        break
      fi
    done < "$tmp/unmapped.txt"
  fi
  if [[ -n "$tolerated" ]]; then
    echo "$tolerated" >> "$tmp/pending.txt"
  else
    echo "$cand" >> "$tmp/real.txt"
  fi
done < "$tmp/candidates.txt"

if [[ -s "$tmp/pending.txt" ]]; then
  echo "note: $ELF spells class names no shrink map covers yet (un-shrunk until the next release cut):" >&2
  sort -u "$tmp/pending.txt" | sed 's/^/    /' >&2
fi

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
if grep -qE "$members" "$tmp/strings4.txt"; then
  echo "ERROR: --shrink image $ELF still spells served member names:" >&2
  grep -E "$members" "$tmp/strings4.txt" | sort -u | sed 's/^/    /' >&2
  echo "A log message or other string literal spelling one of these counts too: reword it." >&2
  exit 1
fi

if [[ -n "$APP_PREFIX" ]]; then
  if grep -qF "$APP_PREFIX" "$tmp/strings6.txt"; then
    echo "ERROR: --shrink-app image $ELF still spells the app package '$APP_PREFIX':" >&2
    grep -F "$APP_PREFIX" "$tmp/strings6.txt" | sort -u | sed 's/^/    /' >&2
    exit 1
  fi
fi

echo "OK: $ELF carries no original Java class, descriptor or served member name${APP_PREFIX:+, nor the app package '$APP_PREFIX'}."
