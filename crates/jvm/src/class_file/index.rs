// SPDX-License-Identifier: GPL-3.0-only
//! Looking a class up by name.
//!
//! The loaded classes come from two class sections — the framework's,
//! embedded in firmware, and the app's, in its PAPK — each carrying an index
//! sorted by name hash (`class-link` refuses a set in which two names hash
//! alike). A lookup is therefore two binary searches over flash, then one
//! byte compare of the hit's name, instead of the hash-then-compare scan
//! over every loaded class it used to be (~8 µs on the RP2350 for ~250
//! classes, on every resolution miss, every `checkcast`, every `new`'s
//! layout walk). Classes appended past both sections — the tests'
//! [`crate::Jvm::load_class`] — sit in a tail that is still scanned.

use core::ops::Deref;

use class_link::{ClassSection, IndexEntry};

use super::{name_hash, ClassFile};

/// Where the loaded classes came from, for lookup by name: framework
/// section entries first (indices `0..app_base`), the app section's next
/// (`app_base..tail_from`), then the tail.
#[derive(Clone, Copy, Debug)]
pub struct ClassIndex {
    fw: &'static [IndexEntry],
    app: &'static [IndexEntry],
    /// Class-table index of the app section's first class.
    app_base: usize,
    /// Class-table index of the first class no section indexes.
    tail_from: usize,
}

impl ClassIndex {
    /// No sections: every class is in the tail, and lookup is the scan.
    pub const LINEAR: ClassIndex = ClassIndex {
        fw: &[],
        app: &[],
        app_base: 0,
        tail_from: 0,
    };

    /// The index over `fw` loaded first, then `app`; either may be absent.
    pub fn new(fw: Option<&ClassSection<'static>>, app: Option<&ClassSection<'static>>) -> Self {
        let fw_len = fw.map_or(0, |s| s.len());
        let app_len = app.map_or(0, |s| s.len());
        ClassIndex {
            fw: fw.map_or(&[], |s| s.index()),
            app: app.map_or(&[], |s| s.index()),
            app_base: fw_len,
            tail_from: fw_len + app_len,
        }
    }

    /// The position `hash` names in a section's index, if any. Hashes are
    /// unique within a section, so a match is at most one entry.
    #[inline]
    fn probe(index: &[IndexEntry], hash: u32) -> Option<usize> {
        index
            .binary_search_by_key(&hash, |e| e.hash)
            .ok()
            .map(|i| index[i].idx as usize)
    }
}

/// The class table with its index: what every lookup by name takes.
/// `Copy`, and dereferences to the table, so `classes[ci]`, `.len()` and
/// `.iter()` read as they always did.
#[derive(Clone, Copy)]
pub struct Classes<'a> {
    pub files: &'a [ClassFile],
    pub index: &'a ClassIndex,
}

impl<'a> Classes<'a> {
    /// A table with no index: tests and single-class runs.
    pub fn linear(files: &'a [ClassFile]) -> Self {
        Self {
            files,
            index: &ClassIndex::LINEAR,
        }
    }
}

impl Classes<'_> {
    /// The class `ci`'s superclass, `None` for a class with no superclass
    /// in the set (Object, a builtin parent, or a parent this board's
    /// framework excludes).
    ///
    /// A class of a packed section carries its superclass's position in
    /// that section (`Link::super_idx`), so a walk up the chain is an index
    /// read per level. Only the step out of a section — an app class whose
    /// parent is a framework class — is a lookup, by the hash the class's
    /// table stores; a framework class has nowhere else to look.
    #[inline]
    pub fn super_of(&self, ci: usize) -> Option<usize> {
        let cf = self.files.get(ci)?;
        let ix = self.index;
        let link = cf.link();
        if ci < ix.tail_from {
            let base = if ci < ix.app_base { 0 } else { ix.app_base };
            if let Some(local) = link.super_idx() {
                return Some(base + local);
            }
            if base == 0 {
                return None;
            }
        }
        let sup = cf.super_class_name()?;
        find_class_hashed(*self, link.super_hash(), sup)
    }
}

impl Deref for Classes<'_> {
    type Target = [ClassFile];

    #[inline]
    fn deref(&self) -> &[ClassFile] {
        self.files
    }
}

/// Index of the registered class named `name`, if any. The one lookup
/// every resolution path funnels through.
#[inline(never)]
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
pub fn find_class(classes: Classes<'_>, name: &[u8]) -> Option<usize> {
    find_class_hashed(classes, name_hash(name), name)
}

/// [`find_class`] for a caller that already has the name's hash (a link
/// table's superclass or interface hash): the sections are searched by
/// hash and the bytes compared once, on the hit.
#[inline(never)]
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
pub fn find_class_hashed(classes: Classes<'_>, hash: u32, name: &[u8]) -> Option<usize> {
    #[cfg(feature = "parity-metrics")]
    crate::parity::count_find_class();
    let ix = classes.index;
    if let Some(i) = ClassIndex::probe(ix.fw, hash) {
        if classes
            .files
            .get(i)
            .is_some_and(|cf| cf.is_named(name, hash))
        {
            return Some(i);
        }
    }
    if let Some(i) = ClassIndex::probe(ix.app, hash) {
        let i = ix.app_base + i;
        if classes
            .files
            .get(i)
            .is_some_and(|cf| cf.is_named(name, hash))
        {
            return Some(i);
        }
    }
    let tail = classes.files.get(ix.tail_from..)?;
    tail.iter()
        .position(|cf| cf.is_named(name, hash))
        .map(|p| p + ix.tail_from)
}
