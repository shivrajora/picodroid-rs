// SPDX-License-Identifier: GPL-3.0-only
//! Arena compaction in bounded slices.
//!
//! The collector slides every live arena span down over the garbage between
//! them, in ascending offset order. It used to gather one `u64` key per live
//! span into a scratch `Vec` first, so the buffer grew with the live set —
//! 38,912 B for 4,864 live object spans — and was asked for as one block at
//! every collection. On a fragmented heap that request fails (the largest
//! free block was 25 KB with 62 KB free), the compaction is skipped, and the
//! arena stays fragmented: the very condition that refused the buffer
//! (docs/designs/claudeusage-gaps-roadmap-2026-09.md, G10).
//!
//! Now the buffer is fixed at [`SLICE_ENTRIES`] keys (4 KB), claimed once at
//! boot, and a compaction that has more live spans than that runs in
//! several passes: each pass selects the next slice of the global order —
//! the smallest keys above the last one slid — and slides just those.
//! That is correct because spans never overlap and each pass handles a
//! prefix of the ascending order: after a pass the write cursor sits at or
//! below the start of the smallest span not yet handled, so a later slide
//! never overwrites data still waiting to move. Every pass rescans the slot
//! store (O(N) per pass, N/K passes) and keeps its slice in a max-heap
//! (O(log K) per candidate); the compaction is already linear in the live
//! set, so a few extra scans cost far less than a skipped compaction.

use alloc::vec::Vec;

/// Keys per slice: 512 × 8 B = 4 KB, sized so an ordinary live set (a few
/// hundred arena spans) compacts in one pass and the largest seen so far
/// (about 5,000) in ten.
pub const SLICE_ENTRIES: usize = 512;

/// The slice a compaction claims for itself when boot did not (unit tests,
/// a heap reset without a pre-reservation): 256 B, small enough for the
/// fixed 8 KB budgets the OOM tests run under. Only the pass count grows.
pub const FALLBACK_ENTRIES: usize = 32;

/// Claim the slice buffer, once, at `entries` keys. Best-effort: on a heap
/// too full for it the buffer stays empty and [`next_slice`] compacts
/// nothing, as the old refusal did — boot reserves the full slice while
/// the heap is young precisely so that this never decides anything.
pub(crate) fn ensure_slice_buf(buf: &mut Vec<u64>, entries: usize) -> bool {
    buf.capacity() > 0 || buf.try_reserve_exact(entries).is_ok()
}

/// Pack one span into the sort key described in [`crate::sort`]: the arena
/// offset in the high half so the order is by offset, then the slot index
/// (which makes every key unique) and the span length.
#[inline]
pub(crate) fn key(offset: u32, slot: usize, len: u16) -> u64 {
    // Slots are addressed by a `u16` ref, so an index always fits.
    debug_assert!(slot <= u16::MAX as usize, "slot index overflows the key");
    ((offset as u64) << 32) | ((slot as u64) << 16) | len as u64
}

/// Inverse of [`key`]: `(slot, offset, len)`.
#[inline]
pub(crate) fn unpack(key: u64) -> (usize, u32, u16) {
    (
        (key >> 16) as usize & 0xffff,
        (key >> 32) as u32,
        key as u16,
    )
}

/// Fill `buf` with the next slice of the ascending key order: the smallest
/// keys from `keys` that are greater than `after` (all of them, from the
/// start, when `after` is `None`), at most `buf.capacity()` of them, sorted.
/// Returns whether the buffer came back full, in which case more keys may
/// remain and the caller runs another pass with `after = buf.last()`; a
/// pass that finds nothing leaves `buf` empty.
///
/// The iterator is `dyn` so the three arena sites share one instantiation
/// (the JVM's flash budget pays for every monomorphisation twice over on
/// the RP2040; see [`crate::sort`]).
pub(crate) fn next_slice(
    buf: &mut Vec<u64>,
    after: Option<u64>,
    keys: &mut dyn Iterator<Item = u64>,
) -> bool {
    buf.clear();
    let cap = buf.capacity();
    if cap == 0 {
        return false;
    }
    // Keep the `cap` smallest candidates in a max-heap rooted at `buf[0]`:
    // a candidate below the root evicts it.
    for k in keys {
        if after.is_some_and(|a| k <= a) {
            continue;
        }
        if buf.len() < cap {
            buf.push(k);
            let last = buf.len() - 1;
            sift_up(buf, last);
        } else if k < buf[0] {
            buf[0] = k;
            sift_down(buf, 0);
        }
    }
    crate::sort::sort_keys(buf);
    buf.len() == cap
}

fn sift_up(h: &mut [u64], mut i: usize) {
    while i > 0 {
        let p = (i - 1) / 2;
        if h[i] <= h[p] {
            return;
        }
        h.swap(i, p);
        i = p;
    }
}

fn sift_down(h: &mut [u64], mut i: usize) {
    let n = h.len();
    loop {
        let l = 2 * i + 1;
        let r = l + 1;
        let mut m = i;
        if l < n && h[l] > h[m] {
            m = l;
        }
        if r < n && h[r] > h[m] {
            m = r;
        }
        if m == i {
            return;
        }
        h.swap(i, m);
        i = m;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// Walk the slices of `keys` with a buffer of `cap` entries and return
    /// the concatenation, which must be the sorted input.
    fn walk(keys: &[u64], cap: usize) -> (Vec<u64>, usize) {
        let mut buf = Vec::with_capacity(cap);
        let mut out = Vec::new();
        let mut after = None;
        let mut passes = 0;
        loop {
            passes += 1;
            let more = next_slice(&mut buf, after, &mut keys.iter().copied());
            out.extend_from_slice(&buf);
            after = buf.last().copied();
            if !more {
                break;
            }
        }
        (out, passes)
    }

    #[test]
    fn slices_concatenate_to_the_sorted_order() {
        let keys: Vec<u64> = (0..97u64).map(|i| (i * 7919) % 97).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        for cap in [1, 2, 3, 16, 96, 97, 98, 512] {
            let (out, passes) = walk(&keys, cap);
            assert_eq!(out, sorted, "cap {cap}");
            // One extra pass only when the last slice happened to be full.
            let expect = keys.len().div_ceil(cap) + usize::from(keys.len() % cap == 0);
            assert_eq!(passes, expect, "cap {cap}");
        }
    }

    #[test]
    fn duplicate_free_keys_are_never_skipped_or_repeated() {
        // Keys sharing an offset differ in the slot bits, so the threshold
        // on the full key keeps them apart.
        let keys = vec![key(0, 0, 0), key(0, 1, 0), key(0, 2, 4), key(5, 3, 1)];
        let (out, _) = walk(&keys, 2);
        assert_eq!(out, keys);
    }

    #[test]
    fn empty_input_and_zero_capacity_yield_nothing() {
        let mut buf = Vec::with_capacity(4);
        assert!(!next_slice(&mut buf, None, &mut core::iter::empty()));
        assert!(buf.is_empty());
        let mut none = Vec::new();
        assert!(!next_slice(&mut none, None, &mut [1u64, 2].into_iter()));
        assert!(none.is_empty());
        assert!(ensure_slice_buf(&mut none, SLICE_ENTRIES));
        assert_eq!(none.capacity(), SLICE_ENTRIES);
        assert!(ensure_slice_buf(&mut none, FALLBACK_ENTRIES));
        assert_eq!(none.capacity(), SLICE_ENTRIES, "a claimed buffer is kept");
    }

    #[test]
    fn key_round_trips() {
        let k = key(0xdead_beef, 0xfedc, 0x1234);
        assert_eq!(unpack(k), (0xfedc, 0xdead_beef, 0x1234));
        assert!(key(1, 0, 0) > key(0, u16::MAX as usize, u16::MAX));
    }
}
