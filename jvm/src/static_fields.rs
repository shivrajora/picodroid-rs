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
