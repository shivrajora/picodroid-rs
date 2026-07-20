// SPDX-License-Identifier: GPL-3.0-only
//! Chunked-slot storage shared by `ObjectHeap.objects` and
//! `ArrayHeap.arrays`. Replaces a single growing `Vec<Option<T>>` whose
//! doubling reallocations forced increasingly large contiguous allocations
//! (capacity 1024 of an `Option<JvmObject>` slot = tens of KB single block,
//! which the FreeRTOS heap couldn't satisfy on Pico Enviro+ once
//! fragmentation set in).
//!
//! With `CHUNK_SIZE`-slot chunks, growth allocates one small fixed-size chunk
//! at a time and previously-allocated chunks aren't moved or freed-and-
//! realloc'd. Worst-case contiguous request is `CHUNK_SIZE * size_of::<Option<T>>()`
//! — tens of KiB max for `JvmArray`, single-digit KiB for most types — which
//! any non-pathological heap state tolerates.
//!
//! API mirrors `Vec<Option<T>>` for the subset the heap modules use:
//! `len()`, `get/get_mut(idx)`, `push(val) -> idx`, `iter()`, plus
//! `Index/IndexMut<usize>` for in-place writes via `slots[idx] = ...`.

use crate::tunables::{CHUNK_MASK, CHUNK_SHIFT, CHUNK_SIZE};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::{Index, IndexMut};

pub struct ChunkedSlots<T> {
    chunks: Vec<Box<[Option<T>]>>,
    /// Highest used index + 1. Mirrors `Vec::len`.
    len: usize,
}

impl<T> ChunkedSlots<T> {
    pub const fn new() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get(&self, idx: usize) -> Option<&Option<T>> {
        if idx >= self.len {
            return None;
        }
        let (c, i) = (idx >> CHUNK_SHIFT, idx & CHUNK_MASK);
        self.chunks.get(c).map(|chunk| &chunk[i])
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut Option<T>> {
        if idx >= self.len {
            return None;
        }
        let (c, i) = (idx >> CHUNK_SHIFT, idx & CHUNK_MASK);
        self.chunks.get_mut(c).map(|chunk| &mut chunk[i])
    }

    /// Append a new slot. Allocates a fresh chunk if the tail chunk is full.
    /// Returns the index of the new slot.
    pub fn push(&mut self, val: Option<T>) -> usize {
        let idx = self.len;
        let (c, i) = (idx >> CHUNK_SHIFT, idx & CHUNK_MASK);
        if c >= self.chunks.len() {
            // Build via Vec → Box<[_]> so the chunk is heap-resident from
            // the start; `Box::new([None; CHUNK_SIZE])` would stack-build
            // the array first, risking a small-task overflow for large T.
            // `resize_with` doesn't require `Option<T>: Clone`.
            let mut v: Vec<Option<T>> = Vec::with_capacity(CHUNK_SIZE);
            v.resize_with(CHUNK_SIZE, || None);
            self.chunks.push(v.into_boxed_slice());
        }
        self.chunks[c][i] = val;
        self.len += 1;
        idx
    }

    /// Structural invariant (mem-diag integrity sweep): the chunk count is
    /// exactly what `len` requires. Chunks are never freed or shrunk, so
    /// ceil(len / CHUNK_SIZE) must equal the chunk count.
    #[cfg(feature = "mem-diag")]
    pub(crate) fn invariant_holds(&self) -> bool {
        let expected = if self.len == 0 {
            0
        } else {
            ((self.len - 1) >> CHUNK_SHIFT) + 1
        };
        self.chunks.len() == expected && self.len <= self.chunks.len() * CHUNK_SIZE
    }

    pub fn iter(&self) -> impl Iterator<Item = &Option<T>> + '_ {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.iter())
            .take(self.len)
    }
}

impl<T> Default for ChunkedSlots<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Index<usize> for ChunkedSlots<T> {
    type Output = Option<T>;
    fn index(&self, idx: usize) -> &Option<T> {
        let (c, i) = (idx >> CHUNK_SHIFT, idx & CHUNK_MASK);
        &self.chunks[c][i]
    }
}

impl<T> IndexMut<usize> for ChunkedSlots<T> {
    fn index_mut(&mut self, idx: usize) -> &mut Option<T> {
        let (c, i) = (idx >> CHUNK_SHIFT, idx & CHUNK_MASK);
        &mut self.chunks[c][i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let s: ChunkedSlots<i32> = ChunkedSlots::new();
        assert_eq!(s.len(), 0);
        assert!(s.get(0).is_none());
    }

    #[test]
    fn push_returns_sequential_indices() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        assert_eq!(s.push(Some(10)), 0);
        assert_eq!(s.push(Some(20)), 1);
        assert_eq!(s.push(Some(30)), 2);
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn push_none_still_advances_len() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        assert_eq!(s.push(None), 0);
        assert_eq!(s.push(None), 1);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0], None);
        assert_eq!(s[1], None);
    }

    #[test]
    fn get_returns_stored_value() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        s.push(Some(42));
        assert_eq!(s.get(0), Some(&Some(42)));
    }

    #[test]
    fn get_out_of_range_returns_none() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        s.push(Some(1));
        assert!(s.get(1).is_none());
        assert!(s.get(99).is_none());
    }

    #[test]
    fn get_mut_allows_in_place_update() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        s.push(Some(1));
        if let Some(slot) = s.get_mut(0) {
            *slot = Some(99);
        }
        assert_eq!(s[0], Some(99));
    }

    #[test]
    fn get_mut_out_of_range_returns_none() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        s.push(Some(1));
        assert!(s.get_mut(1).is_none());
        assert!(s.get_mut(64).is_none());
    }

    #[test]
    fn index_mut_writes_in_place() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        s.push(Some(0));
        s[0] = Some(7);
        assert_eq!(s[0], Some(7));
    }

    /// Critical: indexing arithmetic must work across chunk boundaries.
    /// CHUNK_SIZE = 64, so index 63 → chunk 0, index 64 → chunk 1.
    #[test]
    fn push_crosses_chunk_boundary() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        for i in 0..CHUNK_SIZE {
            assert_eq!(s.push(Some(i as i32)), i);
        }
        assert_eq!(s.len(), CHUNK_SIZE);
        let new_idx = s.push(Some(999));
        assert_eq!(new_idx, CHUNK_SIZE);
        assert_eq!(s[CHUNK_SIZE], Some(999));
        assert_eq!(s[CHUNK_SIZE - 1], Some((CHUNK_SIZE - 1) as i32));
    }

    #[test]
    fn push_across_many_chunks_preserves_values() {
        let mut s: ChunkedSlots<usize> = ChunkedSlots::new();
        let n = CHUNK_SIZE * 5 + 7;
        for i in 0..n {
            assert_eq!(s.push(Some(i)), i);
        }
        assert_eq!(s.len(), n);
        for i in 0..n {
            assert_eq!(s[i], Some(i));
        }
    }

    /// `push` should grow the chunk vector exactly when crossing a boundary,
    /// not earlier — verifies that the fragmentation-friendly growth strategy
    /// described in the module docs actually holds.
    #[test]
    fn push_allocates_one_chunk_per_boundary() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        for i in 0..CHUNK_SIZE {
            s.push(Some(i as i32));
        }
        assert_eq!(s.chunks.len(), 1);
        s.push(Some(0));
        assert_eq!(s.chunks.len(), 2);
        for _ in 1..CHUNK_SIZE {
            s.push(Some(0));
        }
        assert_eq!(s.chunks.len(), 2);
        s.push(Some(0));
        assert_eq!(s.chunks.len(), 3);
    }

    #[test]
    fn iter_yields_only_used_slots() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        for i in 0..3 {
            s.push(Some(i));
        }
        let collected: Vec<_> = s.iter().collect();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0], &Some(0));
        assert_eq!(collected[2], &Some(2));
    }

    /// `iter` must stop at `len`, not at the end of the last chunk — otherwise
    /// GC root scanning would walk uninitialized tail slots.
    #[test]
    fn iter_stops_at_len_not_chunk_end() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        s.push(Some(1));
        s.push(Some(2));
        assert_eq!(s.iter().count(), 2);
    }

    #[test]
    fn iter_walks_multiple_chunks() {
        let mut s: ChunkedSlots<i32> = ChunkedSlots::new();
        let n = CHUNK_SIZE * 2 + 5;
        for i in 0..n {
            s.push(Some(i as i32));
        }
        let collected: Vec<_> = s.iter().collect();
        assert_eq!(collected.len(), n);
        assert_eq!(collected[0], &Some(0));
        assert_eq!(collected[CHUNK_SIZE], &Some(CHUNK_SIZE as i32));
        assert_eq!(collected[n - 1], &Some((n - 1) as i32));
    }

    #[test]
    fn default_matches_new() {
        let a: ChunkedSlots<i32> = ChunkedSlots::default();
        let b: ChunkedSlots<i32> = ChunkedSlots::new();
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn boxed_payload_survives_chunk_growth() {
        let mut s: ChunkedSlots<alloc::boxed::Box<u32>> = ChunkedSlots::new();
        let n = CHUNK_SIZE + 3;
        for i in 0..n {
            s.push(Some(alloc::boxed::Box::new(i as u32)));
        }
        for i in 0..n {
            assert_eq!(s[i].as_deref(), Some(&(i as u32)));
        }
    }
}
