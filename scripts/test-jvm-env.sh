#!/usr/bin/env bash
# Smoke test for `scripts/lib.sh::apply_jvm_env` — verifies that the `[jvm]`
# block in a board.toml is parsed into the right `PICODROID_JVM_*` env vars
# (and that platform-side keys are correctly NOT exported as env vars, since
# the platform build.rs reads them directly from board.toml).
#
# Hooked into `./scripts/pre-commit`. Each failure exits non-zero so the CI
# step fails loudly.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

assert_eq() {
  local name="$1" expected="$2" got="${3-(unset)}"
  if [[ "$got" == "$expected" ]]; then
    echo "  ok:   $name=$got"
  else
    echo "  FAIL: $name=$got (expected $expected)" >&2
    exit 1
  fi
}

assert_unset() {
  # Caller passes the variable's *value* directly (or "" if unset). Using
  # indirect expansion here trips up on some bash versions when the name
  # references an unset variable, so we keep the helper simple.
  local name="$1" val="${2:-}"
  if [[ -z "$val" ]]; then
    echo "  ok:   $name is unset (correct: platform-side or absent)"
  else
    echo "  FAIL: $name=$val (expected unset)" >&2
    exit 1
  fi
}

# Reset between tests so a leak from one case can't mask a bug in the next.
reset_env() {
  unset PICODROID_JVM_GC_ALLOC_THRESHOLD \
        PICODROID_JVM_SLOT_CHUNK_SHIFT \
        PICODROID_JVM_INLINE_ARRAY_DATA \
        PICODROID_JVM_ACTIVITY_STACK_DEPTH \
        PICODROID_JVM_PENDING_OP_QUEUE
}

# Test 1 — all five [jvm] keys present.
echo "==> test 1: all five [jvm] keys present"
cat > "$TMPDIR/full.toml" <<'EOF'
mcu = "rp2350"

[jvm]
gc_alloc_threshold = 128
slot_chunk_shift = 5
inline_array_data = 4
activity_stack_depth = 12
pending_op_queue = 24

[display]
driver = "st7789"
EOF

reset_env
apply_jvm_env "$TMPDIR/full.toml"

# JVM-side keys (the three knobs the jvm crate's build.rs reads from env).
assert_eq PICODROID_JVM_GC_ALLOC_THRESHOLD 128 "${PICODROID_JVM_GC_ALLOC_THRESHOLD:-}"
assert_eq PICODROID_JVM_SLOT_CHUNK_SHIFT 5    "${PICODROID_JVM_SLOT_CHUNK_SHIFT:-}"
assert_eq PICODROID_JVM_INLINE_ARRAY_DATA 4   "${PICODROID_JVM_INLINE_ARRAY_DATA:-}"

# Platform-side keys are consumed directly by platforms/rp/build.rs from
# the parsed BoardConfig; they must NOT be exported as env vars.
assert_unset PICODROID_JVM_ACTIVITY_STACK_DEPTH "${PICODROID_JVM_ACTIVITY_STACK_DEPTH:-}"
assert_unset PICODROID_JVM_PENDING_OP_QUEUE     "${PICODROID_JVM_PENDING_OP_QUEUE:-}"

# Test 2 — no [jvm] block at all → no env vars exported.
echo "==> test 2: board.toml with no [jvm] block leaves env vars unset"
cat > "$TMPDIR/empty.toml" <<'EOF'
mcu = "rp2350"

[display]
driver = "st7789"
EOF

reset_env
apply_jvm_env "$TMPDIR/empty.toml"
assert_unset PICODROID_JVM_GC_ALLOC_THRESHOLD "${PICODROID_JVM_GC_ALLOC_THRESHOLD:-}"
assert_unset PICODROID_JVM_SLOT_CHUNK_SHIFT   "${PICODROID_JVM_SLOT_CHUNK_SHIFT:-}"
assert_unset PICODROID_JVM_INLINE_ARRAY_DATA  "${PICODROID_JVM_INLINE_ARRAY_DATA:-}"

# Test 3 — partial [jvm] block (only one key) → only that env var set.
# Also exercises the lib.sh `_export_jvm_kv` no-match path (previously buggy
# under set -o pipefail).
echo "==> test 3: partial [jvm] block exports only the present keys"
cat > "$TMPDIR/partial.toml" <<'EOF'
mcu = "rp2350"

[jvm]
slot_chunk_shift = 4
EOF

reset_env
apply_jvm_env "$TMPDIR/partial.toml"
assert_eq PICODROID_JVM_SLOT_CHUNK_SHIFT 4 "${PICODROID_JVM_SLOT_CHUNK_SHIFT:-}"
assert_unset PICODROID_JVM_GC_ALLOC_THRESHOLD "${PICODROID_JVM_GC_ALLOC_THRESHOLD:-}"
assert_unset PICODROID_JVM_INLINE_ARRAY_DATA  "${PICODROID_JVM_INLINE_ARRAY_DATA:-}"

# Test 4 — [jvm] block followed by another section → next section's keys
# must not leak in. Guards against a parser regression where the block
# extractor doesn't stop at the next `[`.
echo "==> test 4: [jvm] block stops at the next section header"
cat > "$TMPDIR/bounded.toml" <<'EOF'
mcu = "rp2350"

[jvm]
gc_alloc_threshold = 64

[background_pool]
gc_alloc_threshold = 999
threads = 2
EOF

reset_env
apply_jvm_env "$TMPDIR/bounded.toml"
assert_eq PICODROID_JVM_GC_ALLOC_THRESHOLD 64 "${PICODROID_JVM_GC_ALLOC_THRESHOLD:-}"

echo
echo "==> All apply_jvm_env checks passed."
