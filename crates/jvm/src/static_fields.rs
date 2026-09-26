// SPDX-License-Identifier: GPL-3.0-only
//! Storage for Java static fields, keyed by *(class index, field index)*.
//!
//! A class's statics are prepared together when it is initialised (JVMS
//! §5.5 step 2): [`StaticFieldStore::prepare`] appends one slot per static
//! field, in the class file's declaration order, and records where they
//! start in a per-class table. A `getstatic`/`putstatic` site resolves its
//! `Fieldref` to the declaring class and the field's position in that
//! class's own table (JVMS §5.4.3.2), and from then on addresses the slot
//! by index — the resolution tables remember the index per site.
//!
//! Nothing here holds a name: the store used to be a `Vec` of `(class
//! name, field name, value)` slices compared byte-wise on every miss, 48 B
//! an entry on the host against 32 on the device, and it also read an
//! inherited static under the *using* class's name, so `Sub.X` for an `X`
//! declared on `Super` came back `Null` (M8, docs/parity-audit.md). A slot
//! is now one 16 B `Value` on every target, plus 2 B per registered class
//! and one bit per class for "initialised".
use crate::types::Value;
use alloc::vec::Vec;

/// `base[ci]` for a class whose statics are not prepared.
const UNPREPARED: u16 = u16::MAX;

/// Slack appended per growth so a run of small classes does not reallocate
/// the value table once per class (bounded: at most 512 B idle).
const GROW_SLACK: usize = 32;

/// Process-wide store for Java static fields.
pub struct StaticFieldStore {
    /// One slot per prepared static field, in preparation order.
    values: Vec<Value>,
    /// First slot of each class's statics, indexed by class index;
    /// [`UNPREPARED`] until the class is prepared.
    base: Vec<u16>,
    /// Bitset over class indices: `<clinit>` has run (or been scheduled).
    initialized: Vec<u32>,
}

impl StaticFieldStore {
    pub const fn new() -> Self {
        Self {
            values: Vec::new(),
            base: Vec::new(),
            initialized: Vec::new(),
        }
    }
}

impl Default for StaticFieldStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticFieldStore {
    /// Size the per-class tables for `n` classes, once. `None` when the
    /// heap refuses; the tables then grow on demand from `prepare` and
    /// `mark_initialized`, which may refuse in turn.
    pub fn reserve_classes(&mut self, n: usize) -> Option<()> {
        if n > self.base.len() {
            let extra = n - self.base.len();
            self.base.try_reserve_exact(extra).ok()?;
            self.base.resize(n, UNPREPARED);
        }
        // Manual div_ceil: the crate's MSRV (1.70) predates usize::div_ceil.
        let words = (n + 31) / 32;
        if words > self.initialized.len() {
            let extra = words - self.initialized.len();
            self.initialized.try_reserve_exact(extra).ok()?;
            self.initialized.resize(words, 0);
        }
        Some(())
    }

    /// Returns `true` if the class's `<clinit>` has already run (or been scheduled).
    #[inline]
    pub fn is_initialized(&self, ci: usize) -> bool {
        self.initialized
            .get(ci / 32)
            .is_some_and(|w| w & (1 << (ci % 32)) != 0)
    }

    /// Mark a class as initialized so its `<clinit>` is not re-entered.
    /// `None` when the heap refused the bitset.
    pub fn mark_initialized(&mut self, ci: usize) -> Option<()> {
        self.reserve_classes(ci + 1)?;
        self.initialized[ci / 32] |= 1 << (ci % 32);
        Some(())
    }

    /// Give class `ci` `count` static slots, each `Null`, and return the
    /// first slot's index. Idempotent: a prepared class answers its base.
    /// `None` when the heap refuses the growth (nothing is changed).
    pub fn prepare(&mut self, ci: usize, count: usize) -> Option<u16> {
        self.reserve_classes(ci + 1)?;
        if self.base[ci] != UNPREPARED {
            return Some(self.base[ci]);
        }
        let start = self.values.len();
        let end = start.checked_add(count)?;
        if end > UNPREPARED as usize {
            return None;
        }
        if self.values.capacity() - start < count {
            let want = count.max(GROW_SLACK);
            if self.values.try_reserve_exact(want).is_err() {
                self.values.try_reserve_exact(count).ok()?;
            }
        }
        self.values.resize(end, Value::Null);
        self.base[ci] = start as u16;
        Some(start as u16)
    }

    /// Slot index of static field `fi` (position in the declaring class's
    /// own static-field table) of class `ci`; `None` until the class is
    /// prepared.
    #[inline]
    pub fn slot(&self, ci: usize, fi: usize) -> Option<usize> {
        let base = *self.base.get(ci)?;
        if base == UNPREPARED {
            return None;
        }
        let idx = base as usize + fi;
        (idx < self.values.len()).then_some(idx)
    }

    /// Read a static field by slot index. `Null` out of range.
    #[inline]
    pub fn get_by_index(&self, idx: usize) -> Value {
        self.values.get(idx).copied().unwrap_or(Value::Null)
    }

    /// Write a static field by slot index. A no-op out of range.
    #[inline]
    pub fn set_by_index(&mut self, idx: usize, value: Value) {
        if let Some(v) = self.values.get_mut(idx) {
            *v = value;
        }
    }

    /// Iterate over all stored static field values (for GC root scanning).
    pub fn values_iter(&self) -> impl Iterator<Item = Value> + '_ {
        self.values.iter().copied()
    }

    /// Number of prepared static slots.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_store_is_empty() {
        let s = StaticFieldStore::new();
        assert!(s.is_empty());
        assert!(!s.is_initialized(0));
        assert!(s.slot(0, 0).is_none());
        assert_eq!(s.get_by_index(0), Value::Null);
    }

    #[test]
    fn prepare_gives_null_slots_in_declaration_order() {
        let mut s = StaticFieldStore::new();
        let base = s.prepare(3, 2).unwrap();
        assert_eq!(base, 0);
        assert_eq!(s.slot(3, 0), Some(0));
        assert_eq!(s.slot(3, 1), Some(1));
        assert_eq!(s.get_by_index(0), Value::Null);
        assert_eq!(s.len(), 2);
        // Another class continues after the first.
        assert_eq!(s.prepare(1, 3).unwrap(), 2);
        assert_eq!(s.slot(1, 2), Some(4));
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn prepare_is_idempotent_and_returns_the_same_base() {
        let mut s = StaticFieldStore::new();
        assert_eq!(s.prepare(0, 2).unwrap(), 0);
        assert_eq!(s.prepare(5, 1).unwrap(), 2);
        assert_eq!(s.prepare(0, 2).unwrap(), 0);
        assert_eq!(s.prepare(0, 99).unwrap(), 0, "a prepared class never grows");
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn slot_of_unprepared_class_is_none() {
        let mut s = StaticFieldStore::new();
        s.prepare(2, 1).unwrap();
        assert!(s.slot(0, 0).is_none());
        assert!(s.slot(1, 0).is_none());
        assert!(s.slot(99, 0).is_none());
    }

    #[test]
    fn slot_past_the_table_is_none() {
        let mut s = StaticFieldStore::new();
        s.prepare(0, 2).unwrap();
        // The caller keeps `fi` under the class's own count; the bounds
        // guard only refuses what the table cannot hold at all.
        assert!(s.slot(0, 2).is_none());
    }

    #[test]
    fn prepare_zero_fields() {
        let mut s = StaticFieldStore::new();
        assert_eq!(s.prepare(0, 0).unwrap(), 0);
        assert!(s.slot(0, 0).is_none());
        assert_eq!(s.prepare(1, 1).unwrap(), 0);
        assert_eq!(s.slot(1, 0), Some(0));
    }

    #[test]
    fn get_set_by_index_round_trip() {
        let mut s = StaticFieldStore::new();
        let b = s.prepare(0, 1).unwrap() as usize;
        s.set_by_index(b, Value::Int(42));
        assert_eq!(s.get_by_index(b), Value::Int(42));
        s.set_by_index(b, Value::Int(99));
        assert_eq!(s.get_by_index(b), Value::Int(99));
    }

    /// `set_by_index` is a no-op for out-of-range indices — it must not
    /// panic or grow the value table.
    #[test]
    fn out_of_range_index_is_null_and_noop() {
        let mut s = StaticFieldStore::new();
        s.prepare(0, 1).unwrap();
        s.set_by_index(999, Value::Int(42));
        assert_eq!(s.get_by_index(999), Value::Null);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn initialized_bitset_marks_classes_independently() {
        let mut s = StaticFieldStore::new();
        assert!(!s.is_initialized(0));
        s.mark_initialized(33).unwrap();
        assert!(s.is_initialized(33));
        assert!(!s.is_initialized(32));
        assert!(!s.is_initialized(1));
        assert!(!s.is_initialized(65));
        s.mark_initialized(0).unwrap();
        assert!(s.is_initialized(0));
        assert!(s.is_initialized(33));
    }

    #[test]
    fn mark_initialized_is_idempotent() {
        let mut s = StaticFieldStore::new();
        s.mark_initialized(4).unwrap();
        s.mark_initialized(4).unwrap();
        assert!(s.is_initialized(4));
        assert_eq!(s.initialized.len(), 1);
    }

    #[test]
    fn values_iter_yields_all_values_in_order() {
        let mut s = StaticFieldStore::new();
        s.prepare(0, 2).unwrap();
        s.prepare(1, 1).unwrap();
        s.set_by_index(0, Value::Int(1));
        s.set_by_index(1, Value::Long(2));
        let vals: Vec<Value> = s.values_iter().collect();
        assert_eq!(
            vals,
            alloc::vec![Value::Int(1), Value::Long(2), Value::Null]
        );
    }

    #[test]
    fn values_iter_on_empty_store() {
        let s = StaticFieldStore::new();
        assert_eq!(s.values_iter().count(), 0);
    }

    #[test]
    fn reserve_classes_then_prepare_high_index() {
        let mut s = StaticFieldStore::new();
        s.reserve_classes(300).unwrap();
        assert_eq!(s.base.len(), 300);
        assert_eq!(s.initialized.len(), 10);
        assert_eq!(s.prepare(299, 1).unwrap(), 0);
        assert_eq!(s.slot(299, 0), Some(0));
        // Reserving fewer is a no-op.
        s.reserve_classes(10).unwrap();
        assert_eq!(s.base.len(), 300);
    }

    /// All Value variants must round-trip through the store, since static
    /// fields hold every JVM type.
    #[test]
    fn all_value_kinds_round_trip() {
        let mut s = StaticFieldStore::new();
        let vals = [
            Value::Int(-1),
            Value::Long(i64::MAX),
            Value::Float(1.5),
            Value::Double(2.5),
            Value::Reference(7),
            Value::ObjectRef(11),
            Value::ArrayRef(13),
            Value::Null,
        ];
        let b = s.prepare(0, vals.len()).unwrap() as usize;
        for (i, v) in vals.iter().enumerate() {
            s.set_by_index(b + i, *v);
        }
        for (i, v) in vals.iter().enumerate() {
            assert_eq!(s.get_by_index(b + i), *v);
        }
    }

    /// A heap that refuses the growth leaves the store as it was.
    #[test]
    fn prepare_under_zero_budget_returns_none() {
        use crate::test_alloc::with_budget;
        let mut s = StaticFieldStore::new();
        s.prepare(0, 1).unwrap();
        let r = with_budget(0, || s.prepare(1, 4));
        assert!(r.is_none());
        assert!(s.slot(1, 0).is_none());
        assert_eq!(s.len(), 1);
        // And works again once the heap does.
        assert_eq!(s.prepare(1, 4).unwrap(), 1);
    }

    #[test]
    fn default_equals_new() {
        let a = StaticFieldStore::default();
        let b = StaticFieldStore::new();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.base.len(), b.base.len());
    }
}
