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
//! A key is a [`SiteKey`]: two `u32`s, pointer-free, the same on every
//! target (M8, docs/parity-audit.md). `site` names the resolution site — a
//! constant-pool entry of the calling class, `(class index << 16) | CP
//! index`, which fixes the class, name and descriptor the site spells; or
//! one of the interpreter's own constant-name sites ([`special`]); or, for
//! an upcall from native code, a hash of the name and descriptor whose hit
//! the caller verifies. `recv` is the receiver's runtime class where the
//! answer depends on it (virtual and interface calls, instance fields): an
//! [`ObjectHeap`](crate::object_heap::ObjectHeap) class-table id, or a tag
//! for strings and arrays. Class indices are per class set, so
//! [`ResolveCache::sync`] clears the tables when the set's address or
//! length changes; heap class ids are per heap, so a heap reset clears them
//! too ([`crate::SharedJvmHeap::reset`]).
//!
//! The keys used to be the addresses of the name slices — exact, but twice
//! as wide on the 64-bit simulator, whose tables therefore held half the
//! entries of the device's for the same bytes and hit at a different rate.
//!
//! ## The initialised flag
//!
//! JVMS §5.5 makes `invokestatic`, `getstatic`, `putstatic` and `new`
//! initialise the class first. Once a site's class is known to be
//! initialised that can never become false again for this class set, so
//! the entry remembers it and the instruction skips the probe.
//!
//! ## Native targets
//!
//! A method site stores what it resolved to ([`Target`]), and "no bytecode
//! — ask the handlers" is an answer like any other. Before 2026-09-28 only
//! a found `(class, method)` was stored, so every call on a class with no
//! class file (`String.length()`, `sb.append`, `list.get`, `map.get`: most
//! of what UI code calls) walked the class table three times — the named
//! class, its chain, its interfaces — and an `invokestatic` on one probed
//! the initialised set a fourth time, on every call, ~70 µs on the RP2350.
//! A native entry also carries what the dispatch needs to know without
//! comparing names again ([`flags`]) and, once known, which superclass the
//! native was claimed under (the claim hint), so the handler re-walk is
//! skipped too.

use crate::class_file::ClassFile;
use alloc::vec::Vec;

/// A resolution site, pointer-free. See the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiteKey {
    pub site: u32,
    pub recv: u32,
}

/// `recv` for a site whose target the site alone determines.
pub const RECV_NONE: u32 = 0;
/// `recv` for a string receiver (a `Value::Reference`).
pub const RECV_STRING: u32 = 0x0001_0000;
/// `recv` for an array receiver, `| atype`.
pub const RECV_ARRAY: u32 = 0x0002_0000;
/// `site` prefix of the interpreter's constant-name sites.
const SITE_SPECIAL: u32 = 0xFFFF_0000;
/// `site` prefix of a hashed (name, descriptor) pair: a hit must be
/// verified against the resolved method.
const SITE_HASHED: u32 = 0xFFFE_0000;

/// The interpreter's own resolution sites, one per constant `(name,
/// descriptor)` pair it resolves without a constant-pool entry: the answer
/// depends only on the pair and the receiver's class, so every such site
/// shares the id.
pub mod special {
    /// `toString()Ljava/lang/String;` on an `Object` argument.
    pub const TO_STRING: u16 = 1;
    /// `equals(Ljava/lang/Object;)Z` for the equals-aware collections.
    pub const EQUALS: u16 = 2;
    /// `compareTo(Ljava/lang/Object;)I` for natural-order sorting.
    pub const COMPARE_TO: u16 = 3;
    /// `compare(Ljava/lang/Object;Ljava/lang/Object;)I` on a comparator.
    pub const COMPARE: u16 = 4;
}

impl SiteKey {
    /// The site of constant-pool entry `cp_idx` of class `class_idx`. CP
    /// index 0 is never a valid entry, so no site is 0 — the empty slot.
    #[inline]
    pub const fn cp(class_idx: usize, cp_idx: u16) -> Self {
        debug_assert!(
            class_idx < 0xFFFE,
            "class index collides with the special sites"
        );
        Self {
            site: ((class_idx as u32) << 16) | cp_idx as u32,
            recv: RECV_NONE,
        }
    }

    /// One of the interpreter's [`special`] sites.
    #[inline]
    pub const fn special(id: u16) -> Self {
        Self {
            site: SITE_SPECIAL | id as u32,
            recv: RECV_NONE,
        }
    }

    /// A site for a `(name, descriptor)` pair that has no constant-pool
    /// entry — an upcall from native code. Sixteen bits of hash: a hit
    /// must be checked against the method it names (`helpers::method_matches`)
    /// and a collision is a miss, never a wrong answer.
    #[inline]
    pub fn hashed(name: &str, desc: &str) -> Self {
        let h = crate::class_file::name_hash(name.as_bytes())
            ^ crate::class_file::name_hash(desc.as_bytes()).rotate_left(16);
        Self {
            site: SITE_HASHED | (h >> 16),
            recv: RECV_NONE,
        }
    }

    #[inline]
    pub const fn with_recv(self, recv: u32) -> Self {
        Self {
            site: self.site,
            recv,
        }
    }

    #[inline]
    pub const fn is_hashed(self) -> bool {
        self.site & 0xFFFF_0000 == SITE_HASHED
    }

    /// `recv` for an object whose heap class id is `class_id`.
    #[inline]
    pub const fn recv_object(class_id: u16) -> u32 {
        1 + class_id as u32
    }

    /// `recv` for an array of element type `atype`.
    #[inline]
    pub const fn recv_array(atype: u8) -> u32 {
        RECV_ARRAY | atype as u32
    }
}

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
    /// 12,288 B on every target. claudeusage's four pages touch some 700
    /// method sites; the RP2350 has the RAM to keep most of them, and the
    /// simulator holds the same entries, so it hits at the same rate.
    pub const DEFAULT: Sizes = Sizes {
        methods: 9,
        fields: 8,
        statics: 6,
        classes: 6,
    };
    /// A quarter of the method and field tables: about 3.5 KB.
    pub const SMALL: Sizes = Sizes {
        methods: 7,
        fields: 6,
        statics: 5,
        classes: 5,
    };
}

#[derive(Clone, Copy)]
struct MethodSlot {
    site: u32,
    recv: u32,
    /// A Java target's class index; for a native target, [`NO_HINT`] or
    /// the claim hint's `(name kind << 8) | superclass depth`.
    ci: u16,
    /// A Java target's method index; for a native target with a hint, the
    /// [`NameRef`] index of the class the native was claimed under.
    mi: u16,
    /// [`flags`] bits.
    flags: u8,
    init: bool,
}

#[derive(Clone, Copy)]
struct FieldSlot {
    site: u32,
    recv: u32,
    slot: u16,
}

#[derive(Clone, Copy)]
struct StaticSlot {
    site: u32,
    idx: u16,
}

#[derive(Clone, Copy)]
struct ClassSlot {
    site: u32,
    /// Meaning per `kind`: the class index, or the index into the builtin
    /// or native class-name lists — see [`NameRef`].
    idx: u16,
    kind: u8,
    init: bool,
}

// Loosen only with a parity-audit update: the same bytes on every target.
const _: () = assert!(core::mem::size_of::<MethodSlot>() == 16);
const _: () = assert!(core::mem::size_of::<FieldSlot>() == 12);
const _: () = assert!(core::mem::size_of::<StaticSlot>() == 8);
const _: () = assert!(core::mem::size_of::<ClassSlot>() == 8);

/// Ways per set: a probe compares this many keys.
const WAYS: usize = 4;

/// One four-way set-associative table. A key with `site == 0` is an empty
/// slot: no site the interpreter resolves is 0 (see [`SiteKey::cp`]).
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

    /// First slot of the set `h` selects: the top bits of the mix.
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

    /// Store `slot` in its set: over the way already holding its key
    /// (`same` says which — a re-resolve replaces, never duplicates), else
    /// an empty way (`is_empty`), else the round-robin victim.
    #[inline]
    fn put(
        &mut self,
        h: u32,
        empty: S,
        slot: S,
        same: impl Fn(&S) -> bool,
        is_empty: impl Fn(&S) -> bool,
    ) {
        if !self.ensure(empty) {
            return;
        }
        let b = self.set_base(h);
        let set = &mut self.slots[b..b + WAYS];
        let way = match set
            .iter()
            .position(same)
            .or_else(|| set.iter().position(is_empty))
        {
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

/// Mix a key's two words into a set selector.
#[inline]
fn mix(k: SiteKey) -> u32 {
    let mut h = k.site.wrapping_mul(0x9E37_79B1);
    h ^= k.recv.rotate_left(13).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h.wrapping_mul(0x2C1B_3C6D)
}

const EMPTY_METHOD: MethodSlot = MethodSlot {
    site: 0,
    recv: 0,
    ci: 0,
    mi: 0,
    flags: 0,
    init: false,
};
const EMPTY_FIELD: FieldSlot = FieldSlot {
    site: 0,
    recv: 0,
    slot: 0,
};
const EMPTY_STATIC: StaticSlot = StaticSlot { site: 0, idx: 0 };
const EMPTY_CLASS: ClassSlot = ClassSlot {
    site: 0,
    idx: 0,
    kind: 0,
    init: false,
};

/// Per-site bits a [`MethodHit`] carries, set when the site is inserted.
pub mod flags {
    /// The target has no bytecode: dispatch native (see [`super::Target`]).
    pub const NATIVE: u8 = 1 << 0;
    /// Native dispatch must run the interpreter's own pre-checks first —
    /// the calls it serves before any handler (`getClass`, the
    /// equals-aware collection operations, `Enum.valueOf`,
    /// `ArrayList.sort`). Clear for every other native, which is most.
    pub const PRECHECK: u8 = 1 << 3;
    /// `StringBuilder.append(Object | CharSequence)` / `String.valueOf(Object)`:
    /// the argument's `toString()` runs before the native arm.
    pub const STRINGIFY: u8 = 1 << 4;
    /// `String.format`: the varargs' objects are stringified first.
    pub const FORMAT: u8 = 1 << 5;
}

/// `MethodSlot::ci` of a native target without a claim hint.
const NO_HINT: u16 = u16::MAX;

/// What a method site resolved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Bytecode: class `ci`, method `mi`.
    Java { ci: usize, mi: usize },
    /// No bytecode — a builtin with no class file, or a `native` method:
    /// dispatch to the handlers. `hint` remembers which class the native
    /// was claimed under and how many superclass steps up that is, so the
    /// re-walk is skipped next time (`None`: claimed at depth 0, or not
    /// recorded yet).
    Native { hint: Option<(NameRef, u8)> },
}

/// A resolved method site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodHit {
    pub target: Target,
    /// [`flags`] bits.
    pub flags: u8,
    /// The named class is known to be initialised (invokestatic sites).
    pub init: bool,
}

impl MethodHit {
    /// The `(ci, mi)` of a Java target.
    #[inline]
    pub fn java(&self) -> Option<(usize, usize)> {
        match self.target {
            Target::Java { ci, mi } => Some((ci, mi)),
            Target::Native { .. } => None,
        }
    }
}

/// Where a class's canonical `&'static str` name comes from — the four
/// answers of `helpers::class_name_to_static_in`, as indices so a `new`
/// site's entry holds no pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameRef {
    /// A loaded class file's own name.
    Loaded(u16),
    /// [`crate::native::BUILTIN_CLASS_NAMES`].
    Builtin(u16),
    /// The handler's `native_class_names()`.
    Native(u16),
    /// None of the above: `"unknown"`.
    Unknown,
}

const KIND_LOADED: u8 = 0;
const KIND_BUILTIN: u8 = 1;
const KIND_NATIVE: u8 = 2;
const KIND_UNKNOWN: u8 = 3;

impl NameRef {
    fn pack(self) -> Option<(u16, u8)> {
        Some(match self {
            NameRef::Loaded(i) => (i, KIND_LOADED),
            NameRef::Builtin(i) => (i, KIND_BUILTIN),
            NameRef::Native(i) => (i, KIND_NATIVE),
            NameRef::Unknown => (0, KIND_UNKNOWN),
        })
    }

    fn unpack(idx: u16, kind: u8) -> NameRef {
        match kind {
            KIND_LOADED => NameRef::Loaded(idx),
            KIND_BUILTIN => NameRef::Builtin(idx),
            KIND_NATIVE => NameRef::Native(idx),
            _ => NameRef::Unknown,
        }
    }
}

/// A resolved `new` site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassHit {
    /// Index in the loaded class set, `None` for a builtin.
    pub ci: Option<usize>,
    pub name: NameRef,
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

    /// Empty every table, keeping their allocations.
    pub fn clear(&mut self) {
        self.methods.clear(EMPTY_METHOD);
        self.fields.clear(EMPTY_FIELD);
        self.statics.clear(EMPTY_STATIC);
        self.classes.clear(EMPTY_CLASS);
    }

    /// Bind to `classes`: the tables hold indices into exactly this slice.
    /// A different slice (a new app's `Jvm`, or one that grew) empties them.
    pub fn sync(&mut self, classes: &[ClassFile]) {
        let ptr = classes.as_ptr() as usize;
        let len = classes.len();
        if ptr != self.classes_ptr || len != self.classes_len {
            self.classes_ptr = ptr;
            self.classes_len = len;
            self.clear();
        }
    }

    // ── methods ─────────────────────────────────────────────────────────

    #[cfg_attr(not(feature = "hot-in-ram"), inline)]
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn method(&self, k: SiteKey) -> Option<MethodHit> {
        self.methods
            .set(mix(k))
            .iter()
            .find(|s| s.site == k.site && s.recv == k.recv)
            .map(|s| MethodHit {
                target: if s.flags & flags::NATIVE == 0 {
                    Target::Java {
                        ci: s.ci as usize,
                        mi: s.mi as usize,
                    }
                } else if s.ci == NO_HINT {
                    Target::Native { hint: None }
                } else {
                    Target::Native {
                        hint: Some((NameRef::unpack(s.mi, (s.ci >> 8) as u8), s.ci as u8)),
                    }
                },
                flags: s.flags,
                init: s.init,
            })
    }

    /// A Java target with no flags: what an ordinary bytecode site stores.
    #[inline]
    pub fn insert_method(&mut self, k: SiteKey, ci: usize, mi: usize) {
        self.insert_target(k, Target::Java { ci, mi }, 0);
    }

    /// Store what site `k` resolved to. `flags` are [`flags`] bits;
    /// [`flags::NATIVE`] comes from the target, not the caller.
    #[inline]
    pub fn insert_target(&mut self, k: SiteKey, target: Target, flags: u8) {
        let (ci, mi, flags) = match target {
            Target::Java { ci, mi } => {
                let (Ok(ci16), Ok(mi16)) = (u16::try_from(ci), u16::try_from(mi)) else {
                    return;
                };
                (ci16, mi16, flags & !flags::NATIVE)
            }
            Target::Native { hint: None } => (NO_HINT, 0, flags | flags::NATIVE),
            Target::Native {
                hint: Some((name, depth)),
            } => {
                let Some((idx, kind)) = name.pack() else {
                    return;
                };
                (
                    (kind as u16) << 8 | depth as u16,
                    idx,
                    flags | flags::NATIVE,
                )
            }
        };
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve();
        self.methods.put(
            mix(k),
            EMPTY_METHOD,
            MethodSlot {
                site: k.site,
                recv: k.recv,
                ci,
                mi,
                flags,
                init: false,
            },
            |s| s.site == k.site && s.recv == k.recv,
            |s| s.site == 0,
        );
    }

    /// Remember under which class a native site's call was claimed and how
    /// many superclass steps up that is. A no-op unless the site is cached
    /// as a native target.
    #[inline]
    pub fn mark_claim(&mut self, k: SiteKey, hint: (NameRef, u8)) {
        let Some((idx, kind)) = hint.0.pack() else {
            return;
        };
        if let Some(s) = self
            .methods
            .set_mut(mix(k))
            .iter_mut()
            .find(|s| s.site == k.site && s.recv == k.recv && s.flags & flags::NATIVE != 0)
        {
            s.ci = (kind as u16) << 8 | hint.1 as u16;
            s.mi = idx;
        }
    }

    /// Remember that the class this site names is initialised. A no-op
    /// when the site is not cached (evicted, or the table was refused).
    #[inline]
    pub fn mark_method_init(&mut self, k: SiteKey) {
        if let Some(s) = self
            .methods
            .set_mut(mix(k))
            .iter_mut()
            .find(|s| s.site == k.site && s.recv == k.recv)
        {
            s.init = true;
        }
    }

    // ── instance fields ─────────────────────────────────────────────────

    #[cfg_attr(not(feature = "hot-in-ram"), inline)]
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn field(&self, k: SiteKey) -> Option<usize> {
        self.fields
            .set(mix(k))
            .iter()
            .find(|s| s.site == k.site && s.recv == k.recv)
            .map(|s| s.slot as usize)
    }

    #[inline]
    pub fn insert_field(&mut self, k: SiteKey, slot: usize) {
        let Ok(slot16) = u16::try_from(slot) else {
            return;
        };
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve();
        self.fields.put(
            mix(k),
            EMPTY_FIELD,
            FieldSlot {
                site: k.site,
                recv: k.recv,
                slot: slot16,
            },
            |s| s.site == k.site && s.recv == k.recv,
            |s| s.site == 0,
        );
    }

    // ── static fields ───────────────────────────────────────────────────

    /// Index in the static store for this site. A hit also means the class
    /// was initialised when the entry was made, which it still is.
    #[inline]
    pub fn static_index(&self, k: SiteKey) -> Option<usize> {
        self.statics
            .set(mix(k))
            .iter()
            .find(|s| s.site == k.site)
            .map(|s| s.idx as usize)
    }

    #[inline]
    pub fn insert_static(&mut self, k: SiteKey, idx: usize) {
        let Ok(idx16) = u16::try_from(idx) else {
            return;
        };
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve();
        self.statics.put(
            mix(k),
            EMPTY_STATIC,
            StaticSlot {
                site: k.site,
                idx: idx16,
            },
            |s| s.site == k.site,
            |s| s.site == 0,
        );
    }

    // ── `new` sites ─────────────────────────────────────────────────────

    #[inline]
    pub fn class(&self, k: SiteKey) -> Option<ClassHit> {
        let s = self.classes.set(mix(k)).iter().find(|s| s.site == k.site)?;
        let name = NameRef::unpack(s.idx, s.kind);
        Some(ClassHit {
            ci: match name {
                NameRef::Loaded(i) => Some(i as usize),
                _ => None,
            },
            name,
            init: s.init,
        })
    }

    /// `name` says where the class's canonical name comes from; a loaded
    /// class's index is its `NameRef::Loaded`.
    #[inline]
    pub fn insert_class(&mut self, k: SiteKey, name: NameRef, init: bool) {
        let Some((idx, kind)) = name.pack() else {
            return;
        };
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve();
        self.classes.put(
            mix(k),
            EMPTY_CLASS,
            ClassSlot {
                site: k.site,
                idx,
                kind,
                init,
            },
            |s| s.site == k.site,
            |s| s.site == 0,
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
        let k = SiteKey::cp(3, 7);
        assert_eq!(c.method(k), None);
        c.insert_method(k, 3, 7);
        assert_eq!(
            c.method(k),
            Some(MethodHit {
                target: Target::Java { ci: 3, mi: 7 },
                flags: 0,
                init: false
            })
        );
        c.mark_method_init(k);
        assert!(c.method(k).unwrap().init);
        // The same site on another receiver is another entry.
        assert_eq!(c.method(k.with_recv(SiteKey::recv_object(0))), None);
        // Another class's CP index 7 is another site.
        assert_eq!(c.method(SiteKey::cp(4, 7)), None);
    }

    #[test]
    fn eviction_replaces_never_grows() {
        let mut c = ResolveCache::with_sizes(Sizes {
            methods: 2,
            fields: 2,
            statics: 2,
            classes: 2,
        });
        let keys: Vec<SiteKey> = (1..=8).map(|i| SiteKey::cp(0, i)).collect();
        for (i, k) in keys.iter().enumerate() {
            c.insert_method(*k, i, i);
        }
        assert_eq!(c.methods.slots.len(), 4);
        let cached = keys.iter().filter(|k| c.method(**k).is_some()).count();
        assert_eq!(cached, 4);
        // Every answer is the one stored for that key, never another's.
        for (i, k) in keys.iter().enumerate() {
            if let Some(hit) = c.method(*k) {
                assert_eq!(hit.java(), Some((i, i)));
            }
        }
    }

    #[test]
    fn native_targets_round_trip_with_their_flags_and_claim_hint() {
        let mut c = ResolveCache::new();
        let k = SiteKey::cp(5, 9).with_recv(RECV_STRING);
        c.insert_target(k, Target::Native { hint: None }, flags::PRECHECK);
        let hit = c.method(k).unwrap();
        assert_eq!(hit.target, Target::Native { hint: None });
        assert_eq!(hit.flags, flags::NATIVE | flags::PRECHECK);
        assert_eq!(hit.java(), None);
        assert!(!hit.init);
        // The initialised flag works for a native target too: an
        // `invokestatic` on a builtin skips the probe after its first call.
        c.mark_method_init(k);
        assert!(c.method(k).unwrap().init);
        // A claim hint is packed into the index words and comes back whole.
        c.mark_claim(k, (NameRef::Builtin(7), 2));
        let hit = c.method(k).unwrap();
        assert_eq!(
            hit.target,
            Target::Native {
                hint: Some((NameRef::Builtin(7), 2))
            }
        );
        assert!(hit.init, "marking a claim keeps the initialised flag");
        c.mark_claim(k, (NameRef::Unknown, 0));
        assert_eq!(
            c.method(k).unwrap().target,
            Target::Native {
                hint: Some((NameRef::Unknown, 0))
            }
        );
        // Inserting with a hint directly.
        let j = SiteKey::cp(5, 10);
        c.insert_target(
            j,
            Target::Native {
                hint: Some((NameRef::Loaded(300), 1)),
            },
            0,
        );
        assert_eq!(
            c.method(j).unwrap().target,
            Target::Native {
                hint: Some((NameRef::Loaded(300), 1))
            }
        );
        // A Java target ignores a NATIVE bit a caller passes.
        c.insert_target(
            j,
            Target::Java { ci: 1, mi: 2 },
            flags::NATIVE | flags::STRINGIFY,
        );
        let hit = c.method(j).unwrap();
        assert_eq!(hit.java(), Some((1, 2)));
        assert_eq!(hit.flags, flags::STRINGIFY);
        // A claim on a Java target is a no-op.
        c.mark_claim(j, (NameRef::Builtin(1), 1));
        assert_eq!(c.method(j).unwrap().java(), Some((1, 2)));
    }

    #[test]
    fn sync_clears_on_a_different_class_set() {
        let mut c = ResolveCache::new();
        let a: Vec<ClassFile> = Vec::new();
        c.sync(&a);
        let k = SiteKey::cp(1, 2).with_recv(SiteKey::recv_object(3));
        c.insert_field(k, 5);
        assert_eq!(c.field(k), Some(5));
        c.sync(&a);
        assert_eq!(c.field(k), Some(5));
        let b: Vec<ClassFile> = Vec::with_capacity(1);
        c.sync(&b);
        assert_eq!(c.field(k), None);
    }

    #[test]
    fn clear_empties_every_table() {
        let mut c = ResolveCache::new();
        let k = SiteKey::cp(1, 2);
        c.insert_method(k, 1, 1);
        c.insert_field(k, 1);
        c.insert_static(k, 1);
        c.insert_class(k, NameRef::Loaded(1), true);
        c.clear();
        assert!(c.method(k).is_none());
        assert!(c.field(k).is_none());
        assert!(c.static_index(k).is_none());
        assert!(c.class(k).is_none());
    }

    #[test]
    fn statics_and_classes_round_trip() {
        let mut c = ResolveCache::new();
        let k = SiteKey::cp(2, 9);
        assert_eq!(c.static_index(k), None);
        c.insert_static(k, 9);
        assert_eq!(c.static_index(k), Some(9));
        assert_eq!(c.class(k), None);
        c.insert_class(k, NameRef::Loaded(2), true);
        let hit = c.class(k).unwrap();
        assert_eq!(hit.ci, Some(2));
        assert_eq!(hit.name, NameRef::Loaded(2));
        assert!(hit.init);
        let d = SiteKey::cp(2, 10);
        c.insert_class(d, NameRef::Builtin(4), false);
        let hit = c.class(d).unwrap();
        assert_eq!(hit.ci, None);
        assert_eq!(hit.name, NameRef::Builtin(4));
        assert!(!hit.init);
        c.insert_class(SiteKey::cp(2, 11), NameRef::Unknown, false);
        assert_eq!(c.class(SiteKey::cp(2, 11)).unwrap().name, NameRef::Unknown);
    }

    #[test]
    fn special_and_hashed_sites_are_tagged_and_distinct() {
        let s = SiteKey::special(special::TO_STRING);
        let h = SiteKey::hashed("run", "()V");
        assert!(!s.is_hashed());
        assert!(h.is_hashed());
        assert_ne!(s, h);
        assert_ne!(SiteKey::hashed("run", "()V"), SiteKey::hashed("run", "()I"));
        assert_eq!(SiteKey::hashed("run", "()V"), SiteKey::hashed("run", "()V"));
        // Neither can be the empty slot or a constant-pool site.
        assert_ne!(s.site, 0);
        assert_ne!(h.site, 0);
        assert!(s.site > SiteKey::cp(0xFFFD, 0xFFFF).site);
        assert!(h.site > SiteKey::cp(0xFFFD, 0xFFFF).site);
    }

    #[test]
    fn default_sizes_are_the_same_bytes_on_every_target() {
        let s = Sizes::DEFAULT;
        let bytes = (1usize << s.methods) * core::mem::size_of::<MethodSlot>()
            + (1usize << s.fields) * core::mem::size_of::<FieldSlot>()
            + (1usize << s.statics) * core::mem::size_of::<StaticSlot>()
            + (1usize << s.classes) * core::mem::size_of::<ClassSlot>();
        assert_eq!(bytes, 12_288);
    }
}
