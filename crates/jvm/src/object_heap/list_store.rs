// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;

use crate::types::{Slot, Value};

use super::{reserve_fallible, Exhausted, ObjectHeap};

/// The one-slot cell for a list element. Elements are always references —
/// javac boxes a primitive before `add` — so a bare `long`/`double` is a
/// caller bug, refused as [`Exhausted`] rather than stored torn.
fn elem(v: Value) -> Result<Slot, Exhausted> {
    Slot::from_narrow(v).ok_or(Exhausted)
}

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
        reserve_fallible(&mut self.list_bufs, 1).ok()?;
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
        self.list_bufs
            .get(idx as usize)?
            .as_ref()?
            .get(i)?
            .to_value()
    }

    /// Append `v` to the end of the list. [`Exhausted`] when the buffer
    /// cannot grow; the list is unchanged then.
    pub fn list_add(&mut self, idx: u16, v: Value) -> Result<(), Exhausted> {
        let v = elem(v)?;
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            reserve_fallible(buf, 1)?;
            buf.push(v);
        }
        Ok(())
    }

    /// Insert `v` at position `i`, shifting subsequent elements right.
    /// If `i >= len`, appends to the end. [`Exhausted`] when the buffer
    /// cannot grow; the list is unchanged then.
    pub fn list_insert(&mut self, idx: u16, i: usize, v: Value) -> Result<(), Exhausted> {
        let v = elem(v)?;
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            reserve_fallible(buf, 1)?;
            let pos = i.min(buf.len());
            buf.insert(pos, v);
        }
        Ok(())
    }

    /// Replace the element at position `i` with `v`, returning the old value.
    /// Returns `None` if `i` is out of bounds.
    pub fn list_set(&mut self, idx: u16, i: usize, v: Value) -> Option<Value> {
        let v = Slot::from_narrow(v)?;
        let buf = self.list_bufs.get_mut(idx as usize)?.as_mut()?;
        let old = buf.get(i)?.to_value()?;
        buf[i] = v;
        Some(old)
    }

    /// Remove and return the element at position `i`.
    /// Returns `None` if `i` is out of bounds.
    pub fn list_remove(&mut self, idx: u16, i: usize) -> Option<Value> {
        let buf = self.list_bufs.get_mut(idx as usize)?.as_mut()?;
        if i < buf.len() {
            buf.remove(i).to_value()
        } else {
            None
        }
    }

    /// Remove all elements from the list.
    pub fn list_clear(&mut self, idx: u16) {
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            // Release the buffer, not only the entries: on an arena this
            // small, `clear()` is how an app recovers from an
            // `OutOfMemoryError` (QA 2026-09-13), and a buffer kept for
            // reuse would keep the arena as full as before.
            *buf = Vec::new();
        }
    }

    /// Return an iterator over the list elements.
    pub fn list_iter(&self, idx: u16) -> impl Iterator<Item = Value> + '_ {
        self.list_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map(|v| v.iter().filter_map(|s| s.to_value()))
            .into_iter()
            .flatten()
    }
}
