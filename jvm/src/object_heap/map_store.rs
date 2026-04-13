use alloc::vec::Vec;

use crate::{heap::StringTable, types::Value};

use super::ObjectHeap;

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
            if map_values_eq(*k, key, self, strings) {
                return Some(i);
            }
        }
        None
    }

    /// Put a key-value pair. Returns the previous value if the key existed.
    pub fn map_put(
        &mut self,
        idx: u16,
        key: Value,
        value: Value,
        strings: &StringTable,
    ) -> Option<Value> {
        // Must do the lookup before borrowing mutably.
        let pos = self.map_find_key(idx, key, strings);
        let buf = self.map_bufs.get_mut(idx as usize)?.as_mut()?;
        if let Some(pos) = pos {
            let old = buf[pos].1;
            buf[pos].1 = value;
            Some(old)
        } else {
            buf.push((key, value));
            None
        }
    }

    /// Get the value associated with `key`, or `None` if not found.
    pub fn map_get(&self, idx: u16, key: Value, strings: &StringTable) -> Option<Value> {
        let pos = self.map_find_key(idx, key, strings)?;
        let buf = self.map_bufs.get(idx as usize)?.as_ref()?;
        Some(buf[pos].1)
    }

    /// Remove the entry for `key`. Returns the removed value, or `None`.
    pub fn map_remove(&mut self, idx: u16, key: Value, strings: &StringTable) -> Option<Value> {
        let pos = self.map_find_key(idx, key, strings)?;
        let buf = self.map_bufs.get_mut(idx as usize)?.as_mut()?;
        Some(buf.remove(pos).1)
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
            if map_values_eq(*v, value, self, strings) {
                return true;
            }
        }
        false
    }

    /// Remove all entries from the map.
    pub fn map_clear(&mut self, idx: u16) {
        if let Some(Some(buf)) = self.map_bufs.get_mut(idx as usize) {
            buf.clear();
        }
    }

    /// Return an iterator over the map entries (key, value).
    pub fn map_iter(&self, idx: u16) -> impl Iterator<Item = (Value, Value)> + '_ {
        self.map_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map(|v| v.iter().copied())
            .into_iter()
            .flatten()
    }
}

/// Value equality for map key/value comparison.
/// For ObjectRef values, compares field 0 (wrapper equality for Integer, etc.).
/// For string References with different indices, compares by resolved content.
fn map_values_eq(a: Value, b: Value, objects: &ObjectHeap, strings: &StringTable) -> bool {
    match (a, b) {
        (Value::ObjectRef(ai), Value::ObjectRef(bi)) if ai != bi => {
            let fa = objects.get_field(ai, 0);
            fa.is_some() && fa == objects.get_field(bi, 0)
        }
        (Value::Reference(ai), Value::Reference(bi)) if ai != bi => {
            // String References may have different indices but same content
            // due to StringTable interning behavior after dynamic strings exist.
            let sa = strings.resolve(ai);
            let sb = strings.resolve(bi);
            sa.is_some() && sa == sb
        }
        _ => a == b,
    }
}
