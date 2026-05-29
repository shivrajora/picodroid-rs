// SPDX-License-Identifier: GPL-3.0-only
use crate::types::Value;
use alloc::vec::Vec;

struct StaticEntry {
    class_name: &'static [u8],
    field_name: &'static [u8],
    value: Value,
}

/// Process-wide store for Java static fields.
///
/// Keyed by (class_name, field_name) byte slices backed by Flash (from the
/// class-file constant pool).
pub struct StaticFieldStore {
    entries: Vec<StaticEntry>,
    /// Classes whose `<clinit>` has been executed (or scheduled).
    initialized: Vec<&'static [u8]>,
}

impl StaticFieldStore {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
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
    /// Returns `true` if the class's `<clinit>` has already run (or been scheduled).
    pub fn is_initialized(&self, class_name: &[u8]) -> bool {
        self.initialized.contains(&class_name)
    }

    /// Mark a class as initialized so its `<clinit>` is not re-entered.
    pub fn mark_initialized(&mut self, class_name: &'static [u8]) {
        if !self.is_initialized(class_name) {
            self.initialized.push(class_name);
        }
    }

    /// Read a static field.  Returns `Value::Null` if not yet initialised.
    pub fn get(&self, class_name: &[u8], field_name: &[u8]) -> Value {
        for e in &self.entries {
            if e.class_name == class_name && e.field_name == field_name {
                return e.value;
            }
        }
        Value::Null
    }

    /// Read a static field by cached index.
    #[inline]
    pub fn get_by_index(&self, idx: usize) -> Value {
        self.entries
            .get(idx)
            .map(|e| e.value)
            .unwrap_or(Value::Null)
    }

    /// Write a static field by cached index.
    #[inline]
    pub fn set_by_index(&mut self, idx: usize, value: Value) {
        if let Some(e) = self.entries.get_mut(idx) {
            e.value = value;
        }
    }

    /// Find the index of a static field entry.
    pub fn find_index(&self, class_name: &[u8], field_name: &[u8]) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.class_name == class_name && e.field_name == field_name)
    }

    /// Iterate over all stored static field values (for GC root scanning).
    pub fn values_iter(&self) -> impl Iterator<Item = Value> + '_ {
        self.entries.iter().map(|e| e.value)
    }

    /// Write a static field. Always returns `Some(())`.
    pub fn set(
        &mut self,
        class_name: &'static [u8],
        field_name: &'static [u8],
        value: Value,
    ) -> Option<()> {
        for e in self.entries.iter_mut() {
            if e.class_name == class_name && e.field_name == field_name {
                e.value = value;
                return Some(());
            }
        }
        self.entries.push(StaticEntry {
            class_name,
            field_name,
            value,
        });
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASS_A: &[u8] = b"com/example/A";
    const CLASS_B: &[u8] = b"com/example/B";
    const FIELD_X: &[u8] = b"x";
    const FIELD_Y: &[u8] = b"y";

    #[test]
    fn new_store_is_empty() {
        let s = StaticFieldStore::new();
        assert_eq!(s.get(CLASS_A, FIELD_X), Value::Null);
        assert!(!s.is_initialized(CLASS_A));
        assert!(s.find_index(CLASS_A, FIELD_X).is_none());
    }

    #[test]
    fn unset_field_reads_null() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(1));
        assert_eq!(s.get(CLASS_A, FIELD_Y), Value::Null);
        assert_eq!(s.get(CLASS_B, FIELD_X), Value::Null);
    }

    #[test]
    fn set_then_get_roundtrip() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(42));
        assert_eq!(s.get(CLASS_A, FIELD_X), Value::Int(42));
    }

    #[test]
    fn set_overwrites_existing_entry() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(1));
        s.set(CLASS_A, FIELD_X, Value::Int(99));
        assert_eq!(s.get(CLASS_A, FIELD_X), Value::Int(99));
        assert_eq!(s.find_index(CLASS_A, FIELD_X), Some(0));
    }

    #[test]
    fn set_distinct_fields_creates_distinct_entries() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(1));
        s.set(CLASS_A, FIELD_Y, Value::Int(2));
        s.set(CLASS_B, FIELD_X, Value::Int(3));
        assert_eq!(s.get(CLASS_A, FIELD_X), Value::Int(1));
        assert_eq!(s.get(CLASS_A, FIELD_Y), Value::Int(2));
        assert_eq!(s.get(CLASS_B, FIELD_X), Value::Int(3));
    }

    #[test]
    fn find_index_returns_correct_position() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(10));
        s.set(CLASS_A, FIELD_Y, Value::Int(20));
        s.set(CLASS_B, FIELD_X, Value::Int(30));
        assert_eq!(s.find_index(CLASS_A, FIELD_X), Some(0));
        assert_eq!(s.find_index(CLASS_A, FIELD_Y), Some(1));
        assert_eq!(s.find_index(CLASS_B, FIELD_X), Some(2));
        assert_eq!(s.find_index(CLASS_B, FIELD_Y), None);
    }

    #[test]
    fn get_by_index_returns_stored_value() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(7));
        let idx = s.find_index(CLASS_A, FIELD_X).unwrap();
        assert_eq!(s.get_by_index(idx), Value::Int(7));
    }

    #[test]
    fn get_by_index_out_of_range_returns_null() {
        let s = StaticFieldStore::new();
        assert_eq!(s.get_by_index(0), Value::Null);
        assert_eq!(s.get_by_index(999), Value::Null);
    }

    #[test]
    fn set_by_index_updates_value() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(1));
        let idx = s.find_index(CLASS_A, FIELD_X).unwrap();
        s.set_by_index(idx, Value::Int(123));
        assert_eq!(s.get(CLASS_A, FIELD_X), Value::Int(123));
        assert_eq!(s.get_by_index(idx), Value::Int(123));
    }

    /// `set_by_index` is a no-op for out-of-range indices — it must not panic
    /// or grow the entries vector.
    #[test]
    fn set_by_index_out_of_range_is_noop() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(1));
        s.set_by_index(999, Value::Int(42));
        assert_eq!(s.get(CLASS_A, FIELD_X), Value::Int(1));
        assert_eq!(s.find_index(CLASS_A, FIELD_X), Some(0));
    }

    #[test]
    fn mark_initialized_tracks_class() {
        let mut s = StaticFieldStore::new();
        assert!(!s.is_initialized(CLASS_A));
        s.mark_initialized(CLASS_A);
        assert!(s.is_initialized(CLASS_A));
        assert!(!s.is_initialized(CLASS_B));
    }

    /// Marking a class initialized twice must be idempotent — required to
    /// prevent `<clinit>` re-entry guards from duplicating entries.
    #[test]
    fn mark_initialized_is_idempotent() {
        let mut s = StaticFieldStore::new();
        s.mark_initialized(CLASS_A);
        s.mark_initialized(CLASS_A);
        s.mark_initialized(CLASS_A);
        assert!(s.is_initialized(CLASS_A));
        assert_eq!(s.initialized.len(), 1);
    }

    #[test]
    fn values_iter_yields_all_values_in_order() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(1));
        s.set(CLASS_A, FIELD_Y, Value::Long(2));
        s.set(CLASS_B, FIELD_X, Value::Null);
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
    fn set_preserves_other_entries_when_overwriting() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, FIELD_X, Value::Int(1));
        s.set(CLASS_A, FIELD_Y, Value::Int(2));
        s.set(CLASS_A, FIELD_X, Value::Int(99));
        assert_eq!(s.get(CLASS_A, FIELD_X), Value::Int(99));
        assert_eq!(s.get(CLASS_A, FIELD_Y), Value::Int(2));
        assert_eq!(s.entries.len(), 2);
    }

    /// All Value variants must round-trip through the store, since static
    /// fields hold every JVM type.
    #[test]
    fn all_value_kinds_round_trip() {
        let mut s = StaticFieldStore::new();
        s.set(CLASS_A, b"i", Value::Int(-1));
        s.set(CLASS_A, b"l", Value::Long(i64::MAX));
        s.set(CLASS_A, b"f", Value::Float(1.5));
        s.set(CLASS_A, b"d", Value::Double(2.5));
        s.set(CLASS_A, b"str", Value::Reference(7));
        s.set(CLASS_A, b"obj", Value::ObjectRef(11));
        s.set(CLASS_A, b"arr", Value::ArrayRef(13));
        s.set(CLASS_A, b"n", Value::Null);
        assert_eq!(s.get(CLASS_A, b"i"), Value::Int(-1));
        assert_eq!(s.get(CLASS_A, b"l"), Value::Long(i64::MAX));
        assert_eq!(s.get(CLASS_A, b"f"), Value::Float(1.5));
        assert_eq!(s.get(CLASS_A, b"d"), Value::Double(2.5));
        assert_eq!(s.get(CLASS_A, b"str"), Value::Reference(7));
        assert_eq!(s.get(CLASS_A, b"obj"), Value::ObjectRef(11));
        assert_eq!(s.get(CLASS_A, b"arr"), Value::ArrayRef(13));
        assert_eq!(s.get(CLASS_A, b"n"), Value::Null);
    }

    #[test]
    fn default_equals_new() {
        let a = StaticFieldStore::default();
        let b = StaticFieldStore::new();
        assert_eq!(a.entries.len(), b.entries.len());
        assert_eq!(a.initialized.len(), b.initialized.len());
    }
}
