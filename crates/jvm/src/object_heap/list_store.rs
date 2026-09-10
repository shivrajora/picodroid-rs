// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;

use crate::types::Value;

use super::ObjectHeap;

impl ObjectHeap {
    // ── ArrayList / list_bufs ────────────────────────────────────────────────

    /// Allocate a new list buffer, returning its index.
    /// Reuses a `None` slot (freed by GC) before growing the backing Vec.
    pub fn list_alloc(&mut self) -> Option<u16> {
        if let Some(idx) = self.list_bufs.iter().position(|s| s.is_none()) {
            self.list_bufs[idx] = Some(Vec::new());
            return Some(idx as u16);
        }
        let idx = self.list_bufs.len() as u16;
        self.list_bufs.push(Some(Vec::new()));
        Some(idx)
    }

    /// Free a list buffer slot (GC hook). No-op if `idx` is out of range.
    pub fn list_free(&mut self, idx: u16) {
        if let Some(slot) = self.list_bufs.get_mut(idx as usize) {
            *slot = None;
        }
    }

    /// Return the number of elements in the list.
    pub fn list_len(&self, idx: u16) -> usize {
        self.list_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Return the element at position `i`, or `None` if out of bounds.
    pub fn list_get(&self, idx: u16, i: usize) -> Option<Value> {
        self.list_bufs.get(idx as usize)?.as_ref()?.get(i).copied()
    }

    /// Append `v` to the end of the list.
    pub fn list_add(&mut self, idx: u16, v: Value) {
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            buf.push(v);
        }
    }

    /// Insert `v` at position `i`, shifting subsequent elements right.
    /// If `i >= len`, appends to the end.
    pub fn list_insert(&mut self, idx: u16, i: usize, v: Value) {
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            let pos = i.min(buf.len());
            buf.insert(pos, v);
        }
    }

    /// Replace the element at position `i` with `v`, returning the old value.
    /// Returns `None` if `i` is out of bounds.
    pub fn list_set(&mut self, idx: u16, i: usize, v: Value) -> Option<Value> {
        let buf = self.list_bufs.get_mut(idx as usize)?.as_mut()?;
        let old = *buf.get(i)?;
        buf[i] = v;
        Some(old)
    }

    /// Remove and return the element at position `i`.
    /// Returns `None` if `i` is out of bounds.
    pub fn list_remove(&mut self, idx: u16, i: usize) -> Option<Value> {
        let buf = self.list_bufs.get_mut(idx as usize)?.as_mut()?;
        if i < buf.len() {
            Some(buf.remove(i))
        } else {
            None
        }
    }

    /// Remove all elements from the list.
    pub fn list_clear(&mut self, idx: u16) {
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            buf.clear();
        }
    }

    /// Return an iterator over the list elements.
    pub fn list_iter(&self, idx: u16) -> impl Iterator<Item = Value> + '_ {
        self.list_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map(|v| v.iter().copied())
            .into_iter()
            .flatten()
    }
}
