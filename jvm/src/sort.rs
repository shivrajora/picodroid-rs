// SPDX-License-Identifier: GPL-3.0-only
//! The JVM's one sort.
//!
//! Rust's sort is generic, so every element type sorted with `sort()` or
//! `sort_unstable()` monomorphises the whole sort again. On the RP2040 —
//! whose program region had 1.5 KB spare — the four `Arrays.sort` element
//! types plus the GC's compaction buffer cost tens of kilobytes in near
//! duplicate code.
//!
//! So nothing in the JVM sorts directly. Callers map their elements onto an
//! order-preserving `u64` key, sort here, and map back; every sort in the
//! crate then shares this single instantiation. The transforms below are
//! exact and reversible, so the round trip loses nothing.
//!
//! Unstable is not a compromise here: primitives that compare equal are
//! indistinguishable, and Java's own primitive `Arrays.sort` is a dual-pivot
//! quicksort that makes no stability guarantee either.

/// Below this size, sort with insertion sort to avoid quicksort's setup.
const INSERTION_THRESHOLD: usize = 16;

/// Order-preserving `i64` → `u64` key: flipping the sign bit puts
/// two's-complement negatives below positives under unsigned comparison.
#[inline]
pub(crate) fn key_from_i64(v: i64) -> u64 {
    (v as u64) ^ (1 << 63)
}

#[inline]
pub(crate) fn i64_from_key(k: u64) -> i64 {
    (k ^ (1 << 63)) as i64
}

/// Order-preserving IEEE-754 → key, matching `total_cmp`: negatives invert
/// (their magnitude bits run backwards), positives lift above them. Falls
/// out of this: `-0.0 < +0.0`, and NaNs order consistently instead of
/// making the comparator non-total.
#[inline]
pub(crate) fn key_from_f64_bits(b: u64) -> u64 {
    if b & (1 << 63) != 0 {
        !b
    } else {
        b | (1 << 63)
    }
}

#[inline]
pub(crate) fn f64_bits_from_key(k: u64) -> u64 {
    if k & (1 << 63) != 0 {
        k & !(1 << 63)
    } else {
        !k
    }
}

/// Same transform on 32-bit floats; zero-extending to `u64` preserves the
/// unsigned ordering, so f32 rides the shared sort too.
#[inline]
pub(crate) fn key_from_f32_bits(b: u32) -> u64 {
    let k = if b & (1 << 31) != 0 {
        !b
    } else {
        b | (1 << 31)
    };
    k as u64
}

#[inline]
pub(crate) fn f32_bits_from_key(k: u64) -> u32 {
    let k = k as u32;
    if k & (1 << 31) != 0 {
        k & !(1 << 31)
    } else {
        !k
    }
}

/// Sorts pre-computed keys ascending. Small runs use insertion sort to skip
/// quicksort's setup.
pub(crate) fn sort_keys(buf: &mut [u64]) {
    if buf.len() < INSERTION_THRESHOLD {
        for i in 1..buf.len() {
            let key = buf[i];
            let mut j = i;
            while j > 0 && buf[j - 1] > key {
                buf[j] = buf[j - 1];
                j -= 1;
            }
            buf[j] = key;
        }
    } else {
        buf.sort_unstable();
    }
}
