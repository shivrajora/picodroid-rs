// SPDX-License-Identifier: GPL-3.0-only
//! Persistent, bounded resolution caches.
//!
//! Every invoke, field access and `new` starts from names in a constant
//! pool and has to find a class index, a method index or a field slot. A
//! miss walks the class table and a superclass chain's method tables
//! comparing names, which on an RP2350 running from XIP flash costs some
//! 140 µs per site (claudeusage D4, 2026-09-23: a 60 ms page-turn tick spent
//! 15 ms resolving 109 sites, plus a class-initialised probe per static
//! access). The executor used to memoise into `Vec`s that lived only as
//! long as one top-level invocation — every posted Runnable began cold — and
//! grew by doubling, which is the 10,240-byte request that reset the RP2040
//! under a full heap (`docs/qa-2026-09-13-followups.md` §3).
//!
//! These tables replace that: they live on the shared heap next to the
//! Class-object cache, so a site is resolved once per app run and every
//! thread and every posted Runnable shares the warm result; they are
//! set-associative (four ways), so a probe is one hash and at most four
//! compares rather than a scan of every entry; and each table is allocated
//! once at a fixed size, so nothing here ever grows — a full set evicts one
//! of its ways round-robin, and the cost of an eviction is one re-resolve.
//! If the heap refuses the one allocation, the table stays empty and every
//! probe misses, which is the old declined-cache behaviour with none of the
//! retries.
//!
//! ## Keys
//!
//! Keys are pointer identities of the name slices the interpreter already
//! holds: a constant-pool string inside a loaded class file, a runtime class
//! name from the object heap, or a `names::c` constant. All of these are
//! stable for as long as the class set is loaded. The class set is the
//! `&[ClassFile]` every executor is built over; [`ResolveCache::sync`]
//! compares its address and length on every top-level entry and clears the
//! tables when they change (a new app's `Jvm` is a new table), so a stale
//! index can never be handed to a different class set.
//!
//! ## The initialised flag
//!
//! JVMS §5.5 makes `invokestatic`, `getstatic`, `putstatic` and `new`
//! initialise the class first. The check itself is a linear scan of the
//! initialised-class list by name, and it runs on every one of those
//! instructions. Once a site's class is known to be initialised that can
//! never become false again for this class set, so the entry remembers it
//! and the instruction skips the probe.

use crate::class_file::ClassFile;
use alloc::vec::Vec;

/// log2 of each table's entry count (entries are grouped in sets of four).
/// Sized for the RP2350 by default; the RP2040's 160 KB arena takes
/// [`Sizes::SMALL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sizes {
    pub methods: u8,
    pub fields: u8,
    pub statics: u8,
    pub classes: u8,
}

impl Sizes {
    /// 512 method sites, 256 field sites, 64 static sites, 64 `new` sites:
    /// about 16 KB on a 32-bit target. claudeusage's four pages touch some
    /// 700 method sites; the RP2350 has the RAM to keep most of them.
    ///
    /// A 64-bit host (the simulator) stores each key at twice the width, so
    /// it takes half the entries to hold the same bytes: the simulator's
    /// heap arena models the device's, and a miss costs it microseconds,
    /// not the device's 140 µs.
    #[cfg(target_pointer_width = "32")]
    pub const DEFAULT: Sizes = Sizes {
        methods: 9,
        fields: 8,
        statics: 6,
        classes: 6,
    };
    #[cfg(not(target_pointer_width = "32"))]
    pub const DEFAULT: Sizes = Sizes {
        methods: 8,
        fields: 7,
        statics: 5,
        classes: 5,
    };
    /// A quarter of the method and field tables: about 4.3 KB.
    pub const SMALL: Sizes = Sizes {
        methods: 7,
        fields: 6,
        statics: 5,
        classes: 5,
    };
}

#[derive(Clone, Copy)]
struct MethodSlot {
    k0: usize,
    k1: usize,
    k2: usize,
    ci: u16,
    mi: u16,
    init: bool,
}

#[derive(Clone, Copy)]
struct FieldSlot {
    k0: usize,
    k1: usize,
    k2: usize,
    slot: u16,
}

#[derive(Clone, Copy)]
struct StaticSlot {
    k0: usize,
    k1: usize,
    idx: u16,
}

#[derive(Clone, Copy)]
struct ClassSlot {
    k0: usize,
    /// The class file's own `'static` name (or the builtin's), by address
    /// and length — what `ObjectHeap::alloc_with_defaults` wants.
    name_ptr: usize,
    name_len: u16,
    /// `u16::MAX` when the class is not in the loaded set (a builtin).
    ci: u16,
    init: bool,
}

/// Ways per set: a probe compares this many keys.
const WAYS: usize = 4;

/// One four-way set-associative table. A key with `k0 == 0` is an empty
/// slot: no name the interpreter resolves lives at address zero.
struct Table<S: Copy> {
    slots: Vec<S>,
    shift: u8,
    /// The one allocation was refused; do not ask again.
    refused: bool,
    /// Round-robin victim way for a full set.
    victim: u8,
}

impl<S: Copy> Table<S> {
    const fn new(shift: u8) -> Self {
        Self {
            slots: Vec::new(),
            shift,
            refused: false,
            victim: 0,
        }
    }

    /// Allocate on first use. `false` means the heap said no, once and for
    /// all.
    #[inline]
    fn ensure(&mut self, empty: S) -> bool {
        if !self.slots.is_empty() {
            return true;
        }
        if self.refused {
            return false;
        }
        let n = 1usize << self.shift;
        if self.slots.try_reserve_exact(n).is_err() {
            self.refused = true;
            #[cfg(feature = "parity-metrics")]
            crate::parity::count_cache_decline();
            return false;
        }
        self.slots.resize(n, empty);
        true
    }

    /// First slot of the set `h` selects. Top bits of the mix: the low bits
    /// of a pointer are alignment.
    #[inline]
    fn set_base(&self, h: u32) -> usize {
        ((h >> (32 - self.shift)) as usize) & !(WAYS - 1)
    }

    /// The set's ways, empty when the table was never allocated.
    #[inline]
    fn set(&self, h: u32) -> &[S] {
        if self.slots.is_empty() {
            return &[];
        }
        let b = self.set_base(h);
        &self.slots[b..b + WAYS]
    }

    #[inline]
    fn set_mut(&mut self, h: u32) -> &mut [S] {
        if self.slots.is_empty() {
            return &mut [];
        }
        let b = self.set_base(h);
        &mut self.slots[b..b + WAYS]
    }

    /// Store `slot` in its set: an empty way if there is one (`is_empty`
    /// says which), else the round-robin victim.
    #[inline]
    fn put(&mut self, h: u32, empty: S, slot: S, is_empty: impl Fn(&S) -> bool) {
        if !self.ensure(empty) {
            return;
        }
        let b = self.set_base(h);
        let set = &mut self.slots[b..b + WAYS];
        let way = match set.iter().position(is_empty) {
            Some(w) => w,
            None => {
                let w = self.victim as usize % WAYS;
                self.victim = self.victim.wrapping_add(1);
                w
            }
        };
        set[way] = slot;
    }

    fn clear(&mut self, empty: S) {
        for s in self.slots.iter_mut() {
            *s = empty;
        }
    }

    fn reset(&mut self, shift: u8) {
        self.slots = Vec::new();
        self.shift = shift;
        self.refused = false;
    }
}

#[inline]
fn mix3(a: usize, b: usize, c: usize) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1);
    h ^= (b as u32).rotate_left(13).wrapping_mul(0x85EB_CA77);
    h ^= (c as u32).rotate_left(7).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h.wrapping_mul(0x2C1B_3C6D)
}

const EMPTY_METHOD: MethodSlot = MethodSlot {
    k0: 0,
    k1: 0,
    k2: 0,
    ci: 0,
    mi: 0,
    init: false,
};
const EMPTY_FIELD: FieldSlot = FieldSlot {
    k0: 0,
    k1: 0,
    k2: 0,
    slot: 0,
};
const EMPTY_STATIC: StaticSlot = StaticSlot {
    k0: 0,
    k1: 0,
    idx: 0,
};
const EMPTY_CLASS: ClassSlot = ClassSlot {
    k0: 0,
    name_ptr: 0,
    name_len: 0,
    ci: 0,
    init: false,
};

/// A resolved method site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodHit {
    pub ci: usize,
    pub mi: usize,
    /// The named class is known to be initialised (invokestatic sites).
    pub init: bool,
}

/// A resolved `new` site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassHit {
    /// Index in the loaded class set, `None` for a builtin.
    pub ci: Option<usize>,
    pub name: &'static str,
    pub init: bool,
}

/// The four tables. Lives in [`crate::class_objects::ClassObjectCache`] on
/// the shared heap; see the module docs.
pub struct ResolveCache {
    classes_ptr: usize,
    classes_len: usize,
    methods: Table<MethodSlot>,
    fields: Table<FieldSlot>,
    statics: Table<StaticSlot>,
    classes: Table<ClassSlot>,
}

impl ResolveCache {
    pub const fn new() -> Self {
        Self::with_sizes(Sizes::DEFAULT)
    }

    pub const fn with_sizes(s: Sizes) -> Self {
        Self {
            classes_ptr: 0,
            classes_len: 0,
            methods: Table::new(s.methods),
            fields: Table::new(s.fields),
            statics: Table::new(s.statics),
            classes: Table::new(s.classes),
        }
    }

    /// Resize. Drops whatever is cached; meant for boot, before the first
    /// invocation.
    pub fn configure(&mut self, s: Sizes) {
        self.methods.reset(s.methods);
        self.fields.reset(s.fields);
        self.statics.reset(s.statics);
        self.classes.reset(s.classes);
    }

    /// Bind to `classes`: the tables hold indices into exactly this slice.
    /// A different slice (a new app's `Jvm`, or one that grew) empties them.
    pub fn sync(&mut self, classes: &[ClassFile]) {
        let ptr = classes.as_ptr() as usize;
        let len = classes.len();
        if ptr != self.classes_ptr || len != self.classes_len {
            self.classes_ptr = ptr;
            self.classes_len = len;
            self.methods.clear(EMPTY_METHOD);
            self.fields.clear(EMPTY_FIELD);
            self.statics.clear(EMPTY_STATIC);
            self.classes.clear(EMPTY_CLASS);
        }
    }

    // ── methods ─────────────────────────────────────────────────────────

    #[inline]
    pub fn method(&self, class: &str, name: &str, desc: &str) -> Option<MethodHit> {
        let (k0, k1, k2) = (
            class.as_ptr() as usize,
            name.as_ptr() as usize,
            desc.as_ptr() as usize,
        );
        self.methods
            .set(mix3(k0, k1, k2))
            .iter()
            .find(|s| s.k0 == k0 && s.k1 == k1 && s.k2 == k2)
            .map(|s| MethodHit {
                ci: s.ci as usize,
                mi: s.mi as usize,
                init: s.init,
            })
    }

    #[inline]
    pub fn insert_method(&mut self, class: &str, name: &str, desc: &str, ci: usize, mi: usize) {
        let (Ok(ci16), Ok(mi16)) = (u16::try_from(ci), u16::try_from(mi)) else {
            return;
        };
        let (k0, k1, k2) = (
            class.as_ptr() as usize,
            name.as_ptr() as usize,
            desc.as_ptr() as usize,
        );
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve();
        self.methods.put(
            mix3(k0, k1, k2),
            EMPTY_METHOD,
            MethodSlot {
                k0,
                k1,
                k2,
                ci: ci16,
                mi: mi16,
                init: false,
            },
            |s| s.k0 == 0,
        );
    }

    /// Remember that the class this site names is initialised. A no-op
    /// when the site is not cached (evicted, or the table was refused).
    #[inline]
    pub fn mark_method_init(&mut self, class: &str, name: &str, desc: &str) {
        let (k0, k1, k2) = (
            class.as_ptr() as usize,
            name.as_ptr() as usize,
            desc.as_ptr() as usize,
        );
        if let Some(s) = self
            .methods
            .set_mut(mix3(k0, k1, k2))
            .iter_mut()
            .find(|s| s.k0 == k0 && s.k1 == k1 && s.k2 == k2)
        {
            s.init = true;
        }
    }

    // ── instance fields ─────────────────────────────────────────────────

    #[inline]
    pub fn field(&self, runtime_class: &str, declared: &[u8], name: &[u8]) -> Option<usize> {
        let (k0, k1, k2) = (
            runtime_class.as_ptr() as usize,
            declared.as_ptr() as usize,
            name.as_ptr() as usize,
        );
        self.fields
            .set(mix3(k0, k1, k2))
            .iter()
            .find(|s| s.k0 == k0 && s.k1 == k1 && s.k2 == k2)
            .map(|s| s.slot as usize)
    }

    #[inline]
    pub fn insert_field(&mut self, runtime_class: &str, declared: &[u8], name: &[u8], slot: usize) {
        let Ok(slot16) = u16::try_from(slot) else {
            return;
        };
        let (k0, k1, k2) = (
            runtime_class.as_ptr() as usize,
            declared.as_ptr() as usize,
            name.as_ptr() as usize,
        );
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve();
        self.fields.put(
            mix3(k0, k1, k2),
            EMPTY_FIELD,
            FieldSlot {
                k0,
                k1,
                k2,
                slot: slot16,
            },
            |s| s.k0 == 0,
        );
    }

    // ── static fields ───────────────────────────────────────────────────

    /// Index in the static store for this site. A hit also means the class
    /// was initialised when the entry was made, which it still is.
    #[inline]
    pub fn static_index(&self, class: &[u8], name: &[u8]) -> Option<usize> {
        let (k0, k1) = (class.as_ptr() as usize, name.as_ptr() as usize);
        self.statics
            .set(mix3(k0, k1, 0))
            .iter()
            .find(|s| s.k0 == k0 && s.k1 == k1)
            .map(|s| s.idx as usize)
    }

    #[inline]
    pub fn insert_static(&mut self, class: &[u8], name: &[u8], idx: usize) {
        let Ok(idx16) = u16::try_from(idx) else {
            return;
        };
        let (k0, k1) = (class.as_ptr() as usize, name.as_ptr() as usize);
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve();
        self.statics.put(
            mix3(k0, k1, 0),
            EMPTY_STATIC,
            StaticSlot { k0, k1, idx: idx16 },
            |s| s.k0 == 0,
        );
    }

    // ── `new` sites ─────────────────────────────────────────────────────

    #[inline]
    pub fn class(&self, name: &[u8]) -> Option<ClassHit> {
        let k0 = name.as_ptr() as usize;
        let s = self
            .classes
            .set(mix3(k0, 0, 0))
            .iter()
            .find(|s| s.k0 == k0)?;
        // SAFETY: `name_ptr`/`name_len` were taken from a `&'static str` in
        // `insert_class`, and the table was cleared if the class set that
        // produced it went away (`sync`).
        let name = unsafe {
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                s.name_ptr as *const u8,
                s.name_len as usize,
            ))
        };
        Some(ClassHit {
            ci: (s.ci != u16::MAX).then_some(s.ci as usize),
            name,
            init: s.init,
        })
    }

    #[inline]
    pub fn insert_class(
        &mut self,
        name: &[u8],
        ci: Option<usize>,
        static_name: &'static str,
        init: bool,
    ) {
        let ci16 = match ci {
            None => u16::MAX,
            Some(i) => match u16::try_from(i) {
                Ok(v) if v != u16::MAX => v,
                _ => return,
            },
        };
        let Ok(len16) = u16::try_from(static_name.len()) else {
            return;
        };
        let k0 = name.as_ptr() as usize;
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve();
        self.classes.put(
            mix3(k0, 0, 0),
            EMPTY_CLASS,
            ClassSlot {
                k0,
                name_ptr: static_name.as_ptr() as usize,
                name_len: len16,
                ci: ci16,
                init,
            },
            |s| s.k0 == 0,
        );
    }
}

impl Default for ResolveCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_round_trip_and_init_flag() {
        let mut c = ResolveCache::with_sizes(Sizes {
            methods: 4,
            fields: 4,
            statics: 4,
            classes: 4,
        });
        let class = "a/B";
        let name = "run";
        let desc = "()V";
        assert_eq!(c.method(class, name, desc), None);
        c.insert_method(class, name, desc, 3, 7);
        assert_eq!(
            c.method(class, name, desc),
            Some(MethodHit {
                ci: 3,
                mi: 7,
                init: false
            })
        );
        c.mark_method_init(class, name, desc);
        assert!(c.method(class, name, desc).unwrap().init);
        // Same content at another address is another site.
        let other = alloc::string::String::from("a/B");
        assert_eq!(c.method(&other, name, desc), None);
    }

    #[test]
    fn eviction_replaces_never_grows() {
        let mut c = ResolveCache::with_sizes(Sizes {
            methods: 2,
            fields: 2,
            statics: 2,
            classes: 2,
        });
        let names: Vec<alloc::string::String> = (0..8).map(|i| alloc::format!("m{i}")).collect();
        for (i, n) in names.iter().enumerate() {
            c.insert_method("k", n, "()V", i, i);
        }
        assert_eq!(c.methods.slots.len(), 4);
        let cached = names
            .iter()
            .filter(|n| c.method("k", n, "()V").is_some())
            .count();
        assert_eq!(cached, 4);
        // The four most recent survive: the set is one full round-robin.
        assert!(names[4..].iter().all(|n| c.method("k", n, "()V").is_some()));
    }

    #[test]
    fn sync_clears_on_a_different_class_set() {
        let mut c = ResolveCache::new();
        let a: Vec<ClassFile> = Vec::new();
        c.sync(&a);
        c.insert_field("x", b"y", b"z", 5);
        assert_eq!(c.field("x", b"y", b"z"), Some(5));
        c.sync(&a);
        assert_eq!(c.field("x", b"y", b"z"), Some(5));
        let b: Vec<ClassFile> = Vec::with_capacity(1);
        c.sync(&b);
        assert_eq!(c.field("x", b"y", b"z"), None);
    }

    #[test]
    fn statics_and_classes_round_trip() {
        let mut c = ResolveCache::new();
        assert_eq!(c.static_index(b"C", b"f"), None);
        c.insert_static(b"C", b"f", 9);
        assert_eq!(c.static_index(b"C", b"f"), Some(9));
        assert_eq!(c.class(b"C"), None);
        c.insert_class(b"C", Some(2), "C", true);
        let hit = c.class(b"C").unwrap();
        assert_eq!(hit.ci, Some(2));
        assert_eq!(hit.name, "C");
        assert!(hit.init);
        c.insert_class(b"D", None, "java/lang/Object", false);
        assert_eq!(c.class(b"D").unwrap().ci, None);
    }
}
