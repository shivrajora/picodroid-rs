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
}

impl StaticFieldStore {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl Default for StaticFieldStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticFieldStore {
    /// Read a static field.  Returns `Value::Null` if not yet initialised.
    pub fn get(&self, class_name: &[u8], field_name: &[u8]) -> Value {
        for e in &self.entries {
            if e.class_name == class_name && e.field_name == field_name {
                return e.value;
            }
        }
        Value::Null
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
