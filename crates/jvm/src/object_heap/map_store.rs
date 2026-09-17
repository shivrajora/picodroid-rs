// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;

use crate::{
    heap::StringTable,
    types::{Slot, Value},
};

use super::{reserve_fallible, Exhausted, ObjectHeap};

/// The one-slot cell for a map key or value. Both are always references
/// (or the `Int(1)` marker a `HashSet` stores) — a bare `long`/`double` is a
/// caller bug, refused as [`Exhausted`] rather than stored torn.
fn elem(v: Value) -> Result<Slot, Exhausted> {
    Slot::from_narrow(v).ok_or(Exhausted)
}

/// A stored entry back as values.
fn entry_values(e: &(Slot, Slot)) -> Option<(Value, Value)> {
    Some((e.0.to_value()?, e.1.to_value()?))
}

impl ObjectHeap {
    // ── HashMap / map_bufs ──────────────────────────────────────────────────

    /// Allocate a new map buffer, returning its index.
    /// Reuses a `None` slot (freed by GC) before growing the backing Vec.
    pub fn map_alloc(&mut self) -> Option<u16> {
        if let Some(idx) = self.map_bufs.iter().position(|s| s.is_none()) {
            self.map_bufs[idx] = Some(Vec::new());
            return Some(idx as u16);
        }
        let idx = self.map_bufs.len() as u16;
        reserve_fallible(&mut self.map_bufs, 1).ok()?;
        self.map_bufs.push(Some(Vec::new()));
        Some(idx)
    }

    /// Free a map buffer slot (GC hook). No-op if `idx` is out of range.
    pub fn map_free(&mut self, idx: u16) {
        if let Some(slot) = self.map_bufs.get_mut(idx as usize) {
            *slot = None;
        }
    }

    /// Return the number of entries in the map.
    pub fn map_len(&self, idx: u16) -> usize {
        self.map_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Find the position of `key` in the map buffer using value equality.
    /// Compares ObjectRef by field 0 (wrapper equality) and Reference by
    /// string content (via StringTable) to handle interning non-deduplication.
    fn map_find_key(&self, idx: u16, key: Value, strings: &StringTable) -> Option<usize> {
        let buf = self.map_bufs.get(idx as usize)?.as_ref()?;
        for (i, (k, _)) in buf.iter().enumerate() {
            if k.to_value()
                .is_some_and(|k| map_values_eq(k, key, self, strings))
            {
                return Some(i);
            }
        }
        None
    }

    /// Put a key-value pair. Returns the previous value if the key existed;
    /// [`Exhausted`] when a new entry cannot be stored (the map is unchanged).
    pub fn map_put(
        &mut self,
        idx: u16,
        key: Value,
        value: Value,
        strings: &StringTable,
    ) -> Result<Option<Value>, Exhausted> {
        let (key, value) = (elem(key)?, elem(value)?);
        // Must do the lookup before borrowing mutably.
        let pos = key
            .to_value()
            .and_then(|k| self.map_find_key(idx, k, strings));
        let Some(Some(buf)) = self.map_bufs.get_mut(idx as usize) else {
            return Ok(None);
        };
        if let Some(pos) = pos {
            let old = buf[pos].1.to_value();
            buf[pos].1 = value;
            Ok(old)
        } else {
            reserve_fallible(buf, 1)?;
            buf.push((key, value));
            Ok(None)
        }
    }

    /// The `i`-th entry in iteration order.
    pub fn map_entry_at(&self, idx: u16, i: usize) -> Option<(Value, Value)> {
        entry_values(self.map_bufs.get(idx as usize)?.as_ref()?.get(i)?)
    }

    /// Replace the value of the `i`-th entry, returning the old one.
    pub fn map_set_value_at(&mut self, idx: u16, i: usize, value: Value) -> Option<Value> {
        let value = Slot::from_narrow(value)?;
        let buf = self.map_bufs.get_mut(idx as usize)?.as_mut()?;
        let entry = buf.get_mut(i)?;
        let old = entry.1.to_value();
        entry.1 = value;
        old
    }

    /// Append an entry the caller has already proved absent.
    pub fn map_push(&mut self, idx: u16, key: Value, value: Value) -> Result<(), Exhausted> {
        let (key, value) = (elem(key)?, elem(value)?);
        if let Some(Some(buf)) = self.map_bufs.get_mut(idx as usize) {
            reserve_fallible(buf, 1)?;
            buf.push((key, value));
        }
        Ok(())
    }

    /// Get the value associated with `key`, or `None` if not found.
    pub fn map_get(&self, idx: u16, key: Value, strings: &StringTable) -> Option<Value> {
        let pos = self.map_find_key(idx, key, strings)?;
        let buf = self.map_bufs.get(idx as usize)?.as_ref()?;
        buf[pos].1.to_value()
    }

    /// Remove the entry for `key`. Returns the removed value, or `None`.
    pub fn map_remove(&mut self, idx: u16, key: Value, strings: &StringTable) -> Option<Value> {
        let pos = self.map_find_key(idx, key, strings)?;
        let buf = self.map_bufs.get_mut(idx as usize)?.as_mut()?;
        buf.remove(pos).1.to_value()
    }

    /// Returns `true` if the map contains `key`.
    pub fn map_contains_key(&self, idx: u16, key: Value, strings: &StringTable) -> bool {
        self.map_find_key(idx, key, strings).is_some()
    }

    /// Returns `true` if the map contains `value` (linear scan).
    pub fn map_contains_value(&self, idx: u16, value: Value, strings: &StringTable) -> bool {
        let Some(Some(buf)) = self.map_bufs.get(idx as usize) else {
            return false;
        };
        for (_, v) in buf {
            if v.to_value()
                .is_some_and(|v| map_values_eq(v, value, self, strings))
            {
                return true;
            }
        }
        false
    }

    /// Remove all entries from the map.
    /// Remove the `i`-th entry in iteration order (Iterator.remove on a map
    /// view). Returns false when out of range.
    pub fn map_remove_at(&mut self, idx: u16, i: usize) -> bool {
        if let Some(Some(buf)) = self.map_bufs.get_mut(idx as usize) {
            if i < buf.len() {
                buf.remove(i);
                return true;
            }
        }
        false
    }

    pub fn map_clear(&mut self, idx: u16) {
        if let Some(Some(buf)) = self.map_bufs.get_mut(idx as usize) {
            // Release the buffer, not only the entries: on an arena this
            // small, `clear()` is how an app recovers from an
            // `OutOfMemoryError` (QA 2026-09-13), and a buffer kept for
            // reuse would keep the arena as full as before.
            *buf = Vec::new();
        }
    }

    /// Return an iterator over the map entries (key, value).
    pub fn map_iter(&self, idx: u16) -> impl Iterator<Item = (Value, Value)> + '_ {
        self.map_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map(|v| v.iter().filter_map(entry_values))
            .into_iter()
            .flatten()
    }
}

/// Value equality for map key/value comparison: see [`super::key_eq`].
fn map_values_eq(a: Value, b: Value, objects: &ObjectHeap, strings: &StringTable) -> bool {
    super::key_eq(a, b, objects, strings)
}
