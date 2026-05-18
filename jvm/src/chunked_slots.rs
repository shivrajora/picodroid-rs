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

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::{Index, IndexMut};

const CHUNK_SHIFT: u8 = 6;
const CHUNK_SIZE: usize = 1 << CHUNK_SHIFT;
const CHUNK_MASK: usize = CHUNK_SIZE - 1;

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
