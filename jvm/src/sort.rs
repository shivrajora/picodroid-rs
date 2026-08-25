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

/// Sorts pre-computed keys ascending.
///
/// This is a hand-written introsort rather than `sort_unstable`. Even reduced
/// to a single `u64` instantiation, the standard library's driftsort/ipnsort
/// links 7,386 bytes across five symbols — quicksort, ipnsort, heapsort,
/// median3_rec and the smallsort ordering-violation panic. That buys
/// pattern-defeating behaviour and best-in-class constants that this crate
/// cannot spend flash on: the RP2040 program region runs at 96% and the
/// JVM's two sort callers are `Arrays.sort` (app-sized arrays) and GC arena
/// compaction (called once per collection — 403 times in one `benchmark`
/// run), neither of which is bound by sort throughput.
///
/// Introsort keeps the worst case at O(n log n), which is the property that
/// actually matters here: compaction runs on every GC, so a quadratic blowup
/// on an adversarial arena layout would be a latency cliff, not a slow sort.
pub(crate) fn sort_keys(buf: &mut [u64]) {
    // 2*floor(log2(n)) is the standard introsort budget: enough depth for
    // well-behaved input, low enough that a pathological pivot sequence hits
    // the heapsort fallback before the stack grows.
    let depth = 2 * (usize::BITS - buf.len().leading_zeros()) as usize;
    introsort(buf, depth);
}

fn introsort(mut buf: &mut [u64], mut depth: usize) {
    loop {
        if buf.len() < INSERTION_THRESHOLD {
            insertion_sort(buf);
            return;
        }
        if depth == 0 {
            heapsort(buf);
            return;
        }
        depth -= 1;

        let mid = hoare_partition(buf, median_of_three(buf));
        // Recurse into the smaller side and iterate on the larger, so stack
        // depth stays O(log n) no matter how lopsided the partitions are.
        let (left, right) = buf.split_at_mut(mid + 1);
        if left.len() < right.len() {
            introsort(left, depth);
            buf = right;
        } else {
            introsort(right, depth);
            buf = left;
        }
    }
}

fn insertion_sort(buf: &mut [u64]) {
    for i in 1..buf.len() {
        let key = buf[i];
        let mut j = i;
        while j > 0 && buf[j - 1] > key {
            buf[j] = buf[j - 1];
            j -= 1;
        }
        buf[j] = key;
    }
}

/// Median of first / middle / last. Cheap protection against the sorted and
/// reverse-sorted inputs that make a naive pivot quadratic — and GC
/// compaction feeds this nearly-sorted spans on every collection.
fn median_of_three(buf: &[u64]) -> u64 {
    let (a, b, c) = (buf[0], buf[buf.len() / 2], buf[buf.len() - 1]);
    if a < b {
        if b < c {
            b
        } else if a < c {
            c
        } else {
            a
        }
    } else if a < c {
        a
    } else if b < c {
        c
    } else {
        b
    }
}

/// Hoare partition. Chosen over Lomuto specifically because it splits an
/// all-equal run down the middle instead of degrading to O(n^2) — `Arrays.sort`
/// on a constant array is an entirely ordinary thing for an app to do.
///
/// The pivot is always an element of `buf` (it came from `median_of_three`),
/// so both scans are guaranteed to stop on it and neither can run off the end.
fn hoare_partition(buf: &mut [u64], pivot: u64) -> usize {
    let mut i = 0usize;
    let mut j = buf.len() - 1;
    loop {
        while buf[i] < pivot {
            i += 1;
        }
        while buf[j] > pivot {
            j -= 1;
        }
        if i >= j {
            return j;
        }
        buf.swap(i, j);
        i += 1;
        // j cannot be 0 here: buf[j] == pivot after the swap, and buf[i] would
        // have stopped at or before it, so i >= j would have returned already.
        j -= 1;
    }
}

fn heapsort(buf: &mut [u64]) {
    let len = buf.len();
    for root in (0..len / 2).rev() {
        sift_down(buf, root, len);
    }
    for end in (1..len).rev() {
        buf.swap(0, end);
        sift_down(buf, 0, end);
    }
}

fn sift_down(buf: &mut [u64], mut root: usize, len: usize) {
    loop {
        let mut child = 2 * root + 1;
        if child >= len {
            return;
        }
        if child + 1 < len && buf[child + 1] > buf[child] {
            child += 1;
        }
        if buf[root] >= buf[child] {
            return;
        }
        buf.swap(root, child);
        root = child;
    }
}

#[cfg(test)]
mod tests {
    //! Guards for the hand-written introsort that replaced `sort_unstable`.
    //!
    //! The element transforms are covered by the `Arrays.sort` tests in
    //! `native::tests`; these cover the sort itself, and specifically the
    //! inputs that separate a correct introsort from a plausible one:
    //! all-equal runs (quadratic under Lomuto), sorted and reverse-sorted
    //! runs (quadratic under a naive pivot), and lengths straddling
    //! `INSERTION_THRESHOLD` and the heapsort fallback.

    use super::*;

    extern crate alloc;
    use alloc::vec::Vec;

    /// Deterministic xorshift — no `rand` dependency in a `no_std` crate, and
    /// a fixed seed keeps a failure reproducible.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    fn check(v: &[u64]) {
        let mut got: Vec<u64> = v.to_vec();
        sort_keys(&mut got);
        let mut want: Vec<u64> = v.to_vec();
        want.sort_unstable();
        assert_eq!(got, want, "input {v:?}");
    }

    #[test]
    fn empty_and_single() {
        check(&[]);
        check(&[42]);
    }

    #[test]
    fn every_length_across_both_thresholds() {
        // 0..64 covers the insertion path, the crossover at 16, and enough
        // quicksort levels to recurse.
        let mut rng = Rng(0x243f_6a88_85a3_08d3);
        for len in 0..64usize {
            let v: Vec<u64> = (0..len).map(|_| rng.next() % 1000).collect();
            check(&v);
        }
    }

    #[test]
    fn all_equal_does_not_degrade() {
        // Hoare splits this down the middle; Lomuto would go quadratic.
        for len in [16usize, 17, 64, 500] {
            let v = alloc::vec![7u64; len];
            check(&v);
        }
    }

    #[test]
    fn sorted_and_reverse_sorted() {
        for len in [16usize, 100, 1000] {
            let asc: Vec<u64> = (0..len as u64).collect();
            check(&asc);
            let desc: Vec<u64> = (0..len as u64).rev().collect();
            check(&desc);
        }
    }

    #[test]
    fn nearly_sorted_is_the_gc_compaction_shape() {
        // Arena compaction feeds spans that are already in offset order
        // except for the holes freed by the last sweep.
        let mut v: Vec<u64> = (0..500u64).map(|i| i * 8).collect();
        let mut rng = Rng(1);
        for _ in 0..20 {
            let a = (rng.next() % 500) as usize;
            let b = (rng.next() % 500) as usize;
            v.swap(a, b);
        }
        check(&v);
    }

    #[test]
    fn many_duplicates() {
        let mut rng = Rng(99);
        let v: Vec<u64> = (0..1000).map(|_| rng.next() % 5).collect();
        check(&v);
    }

    #[test]
    fn extremes_and_full_range() {
        check(&[u64::MAX, 0, u64::MAX, 0, 1, u64::MAX - 1]);
        let mut rng = Rng(0xdead_beef);
        let v: Vec<u64> = (0..300).map(|_| rng.next()).collect();
        check(&v);
    }

    #[test]
    fn heapsort_fallback_is_correct() {
        // Drive the fallback directly; the depth budget makes it unreachable
        // from sort_keys on any input this test could construct.
        let mut rng = Rng(7);
        for len in [1usize, 2, 3, 17, 64, 257] {
            let v: Vec<u64> = (0..len).map(|_| rng.next() % 100).collect();
            let mut got = v.clone();
            heapsort(&mut got);
            let mut want = v.clone();
            want.sort_unstable();
            assert_eq!(got, want, "heapsort len {len}");
        }
    }

    #[test]
    fn introsort_at_zero_depth_still_sorts() {
        let mut rng = Rng(11);
        let mut v: Vec<u64> = (0..200).map(|_| rng.next() % 1000).collect();
        let mut want = v.clone();
        introsort(&mut v, 0);
        want.sort_unstable();
        assert_eq!(v, want);
    }

    #[test]
    fn large_random_matches_stdlib() {
        let mut rng = Rng(0x0123_4567_89ab_cdef);
        let v: Vec<u64> = (0..5000).map(|_| rng.next()).collect();
        check(&v);
    }
}
