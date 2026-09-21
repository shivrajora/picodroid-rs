// SPDX-License-Identifier: GPL-3.0-only
//! The JLS §5.1.7 boxed-value cache: `valueOf` of a small integral value
//! returns the same object every time, so `==` on autoboxed constants holds.

use super::*;

// ── JLS §5.1.7 boxed-value cache ─────────────────────────────────────────

/// Slot value meaning "no cached box yet".
pub(super) const BOX_NONE: u16 = u16::MAX;
/// One table per integral wrapper: Integer, Long, Short, Byte, Character.
pub(super) const BOX_TABLES: usize = 5;

impl ObjectHeap {
    fn box_table(class: &str) -> Option<usize> {
        match class {
            c::java_lang_Integer => Some(0),
            c::java_lang_Long => Some(1),
            c::java_lang_Short => Some(2),
            c::java_lang_Byte => Some(3),
            c::java_lang_Character => Some(4),
            _ => None,
        }
    }

    /// Table slot for a value `valueOf` must share: -128..=127 for the
    /// integral wrappers, 0..=127 for `char`.
    fn box_slot(table: usize, v: Value) -> Option<usize> {
        match (table, v) {
            (1, Value::Long(l)) if (-128..=127).contains(&l) => Some((l + 128) as usize),
            (4, Value::Int(i)) if (0..=127).contains(&i) => Some(i as usize),
            (0 | 2 | 3, Value::Int(i)) if (-128..=127).contains(&i) => Some((i + 128) as usize),
            _ => None,
        }
    }

    /// The cached box for `v` of wrapper `class`, if one was recorded.
    pub fn cached_box(&self, class: &str, v: Value) -> Option<u16> {
        if class == c::java_lang_Boolean {
            let Value::Int(b) = v else {
                return None;
            };
            let s = self.bool_cache[(b != 0) as usize];
            return (s != BOX_NONE).then_some(s);
        }
        let t = Self::box_table(class)?;
        let slot = Self::box_slot(t, v)?;
        let s = *self.boxed_cache[t].as_ref()?.get(slot)?;
        (s != BOX_NONE).then_some(s)
    }

    /// Record `idx` as the shared box for `v` of `class` when the JLS wants
    /// one. Best effort: a table that cannot be allocated leaves the value
    /// uncached, which is merely the pre-cache behaviour.
    pub fn cache_box(&mut self, class: &str, v: Value, idx: u16) {
        if class == c::java_lang_Boolean {
            if let Value::Int(b) = v {
                self.bool_cache[(b != 0) as usize] = idx;
            }
            return;
        }
        let Some(t) = Self::box_table(class) else {
            return;
        };
        let Some(slot) = Self::box_slot(t, v) else {
            return;
        };
        if self.boxed_cache[t].is_none() {
            let mut table: Vec<u16> = Vec::new();
            if table.try_reserve_exact(256).is_err() {
                return;
            }
            table.resize(256, BOX_NONE);
            self.boxed_cache[t] = Some(table);
        }
        if let Some(s) = self.boxed_cache[t].as_mut().and_then(|t| t.get_mut(slot)) {
            *s = idx;
        }
    }

    /// Every cached box, plus the `OutOfMemoryError` reserve — GC roots, so
    /// a shared box never dies and the reserve is there when needed.
    pub fn boxed_cache_roots(&self) -> impl Iterator<Item = u16> + '_ {
        self.boxed_cache
            .iter()
            .flatten()
            .flat_map(|t| t.iter().copied())
            .chain(self.bool_cache.iter().copied())
            .chain(core::iter::once(self.oom_reserve))
            .filter(|&s| s != BOX_NONE)
    }

    /// Set aside an `OutOfMemoryError` while the heap can spare one. Idempotent.
    pub fn ensure_oom_reserve(&mut self) {
        if self.oom_reserve == BOX_NONE {
            if let Some(idx) = self.alloc(c::java_lang_OutOfMemoryError) {
                self.oom_reserve = idx;
            }
        }
    }

    /// The reserved `OutOfMemoryError`. It stays reserved — and rooted — so
    /// a heap that cannot allocate anything throws the same object every
    /// time, as HotSpot's preallocated error does; an app that catches it
    /// and keeps allocating still sees `OutOfMemoryError`, never a hard stop.
    pub fn oom_reserve(&self) -> Option<u16> {
        (self.oom_reserve != BOX_NONE).then_some(self.oom_reserve)
    }
}
