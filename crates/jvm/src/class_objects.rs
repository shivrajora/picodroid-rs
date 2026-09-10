// SPDX-License-Identifier: GPL-3.0-only
//! Cache of `java.lang.Class` heap objects, one per class loaded into the JVM.
//!
//! `MyClass.class` is encoded as `ldc CONSTANT_Class` in bytecode. The
//! interpreter resolves it to a `Value::ObjectRef` pointing to a `Class`
//! instance whose `String name` field holds the JVM-internal class name
//! (`"java/lang/String"` etc.). Because users compare these objects with `==`
//! (e.g. `intent.getTargetClass() == MyClass.class`), every `ldc` for the same
//! class must yield the same object — hence this cache.
//!
//! The cache lives on [`crate::SharedJvmHeap`] (process-wide) so that worker
//! threads with their own [`crate::Jvm`] still see one canonical Class object
//! per class — matching the JVM spec's notion of process-wide Class identity.
//!
//! ## Cache key
//!
//! The key is the [`crate::heap::StringTable`] index of the interned class
//! name. `StringTable::intern` deduplicates by content, so the same class
//! name from any Flash-backed class file (or any thread's `Jvm`) maps to
//! the same `u16`. This gives canonical process-wide identity that simple
//! pointer equality can't — two class files referencing `"java/lang/String"`
//! land at different Flash addresses, but they share the interned index.

use alloc::vec::Vec;

/// One cached `(interned class-name index, Class object reference)` pair.
pub struct ClassObjectCache {
    entries: Vec<(u16, u16)>,
}

impl ClassObjectCache {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Returns the cached Class object reference for the class whose name
    /// is interned at `name_idx` in the `StringTable`.
    pub fn lookup(&self, name_idx: u16) -> Option<u16> {
        for &(k, r) in &self.entries {
            if k == name_idx {
                return Some(r);
            }
        }
        None
    }

    /// Insert a `(name_idx, obj_ref)` pair. Caller must have verified there
    /// is no existing entry for `name_idx`.
    pub fn insert(&mut self, name_idx: u16, obj_ref: u16) {
        self.entries.push((name_idx, obj_ref));
    }

    /// Iterate cached object references. Used by the GC to mark each cached
    /// Class object as a strong root.
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        self.entries.iter().map(|&(_, r)| r)
    }
}

impl Default for ClassObjectCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_misses_on_empty_cache() {
        let c = ClassObjectCache::new();
        assert_eq!(c.lookup(7), None);
    }

    #[test]
    fn insert_then_lookup_returns_obj_ref() {
        let mut c = ClassObjectCache::new();
        c.insert(7, 42);
        assert_eq!(c.lookup(7), Some(42));
        assert_eq!(c.lookup(8), None);
    }

    #[test]
    fn iter_yields_each_obj_ref() {
        let mut c = ClassObjectCache::new();
        c.insert(7, 1);
        c.insert(8, 7);
        let collected: alloc::vec::Vec<u16> = c.iter().collect();
        assert_eq!(collected, alloc::vec![1, 7]);
    }
}
