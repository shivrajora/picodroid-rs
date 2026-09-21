// SPDX-License-Identifier: GPL-3.0-only
pub(crate) mod iter_store;
mod lambda;
mod list_store;
mod map_store;
mod sb_store;

use crate::chunked_slots::ChunkedSlots;
use crate::class_file::ClassFile;
use crate::names::c;
use crate::types::{default_for_descriptor, Slot, Value};
use alloc::vec::Vec;

/// Chunked-slot storage for `Option<JvmObject>`. See [`crate::chunked_slots`].
type ChunkedObjects = ChunkedSlots<JvmObject>;

/// Number of implicit fields in `java/lang/Enum` (name + ordinal).
const ENUM_IMPLICIT_FIELDS: usize = 2;

/// Pre-allocation hint for [`ObjectHeap::alloc`], in field *slots*. Native
/// handlers that create view / wrapper objects know exactly how many slots
/// they will write — feed that count back to the heap so the backing span is
/// sized once instead of reallocating inside [`ObjectHeap::set_field`].
/// Unlisted classes default to 0; the span still grows lazily, just at the
/// cost of one extra reallocation.
fn default_field_count_for_native(class_name: &str) -> usize {
    match class_name {
        // Boxed wrappers store the unboxed value at slot 0 — two slots for
        // the category-2 boxes.
        c::java_lang_Long | c::java_lang_Double => 2,
        c::java_lang_Integer
        | c::java_lang_Boolean
        | c::java_lang_Float
        | c::java_lang_Character
        | c::java_lang_Byte
        | c::java_lang_Short => 1,
        // HashMap views store the backing map_buf index at slot 0.
        // Views: map buffer at slot 0, the owning map object at slot 1 (GC pin).
        c::java_util_HashMap_KeySet
        | c::java_util_HashMap_Values
        | c::java_util_HashMap_EntrySet => 2,
        // Map$Entry objects yielded by entrySet(): key at slot 0, value at 1.
        c::java_util_Map_Entry => 2,
        // StringBuilder stores its backing sb_buf index at slot 0.
        c::java_lang_StringBuilder => 1,
        // Random: a two-slot `long` seed, then a two-slot cached gaussian
        // (`native/random.rs` names the slots).
        c::java_util_Random => crate::native::random::FIELD_SLOTS,
        _ => 0,
    }
}

/// Pointer-free object descriptor: field storage lives in
/// [`ObjectHeap::fields_arena`] and is addressed by offset, so the layout is
/// identical (12 B per `Option<JvmObject>` slot) on 32-bit devices and
/// 64-bit hosts — the OBJ-01 host-inflation divergence deleted at the
/// source rather than compensated in reporting (docs/parity-audit.md M6).
#[derive(Clone, Copy)]
pub struct JvmObject {
    /// Offset of this object's field span in [`ObjectHeap::fields_arena`].
    /// Meaningless when `fields_cap == 0`. Updated by GC compaction.
    fields_off: u32,
    /// Index into [`ObjectHeap::class_table`]. Resolves back to the canonical
    /// `&'static str` class name via [`ObjectHeap::class_name`]; storing only
    /// the index here saves 14 B per object versus a direct `&'static str`.
    class_idx: u16,
    /// High-water mark of explicit `set_field` writes, in slots — preserves
    /// the JVMS "uninitialised slot reads as None" contract that the
    /// `alloc_without_defaults_still_leaves_slots_unset` regression test
    /// in `interpreter/tests/fields.rs` enforces. May be less than
    /// `fields_cap`.
    field_count: u8,
    /// Allocated span length in the fields arena, in slots (a `long` or
    /// `double` field is two). `field_count` is u8, so u8 capacity loses
    /// nothing; 255 slots is far above any SDK class.
    fields_cap: u8,
}

// Compile-time guard against future regressions: the descriptor is
// pointer-free, so the slot size is 12 B on EVERY target — the property
// M6 exists to provide. Loosen only with a parity-audit update.
const _: () = assert!(core::mem::size_of::<Option<JvmObject>>() == 12);

/// What a lambda proxy's SAM invocation runs.
#[derive(Clone, Copy)]
pub enum LambdaTarget {
    /// A bytecode body fixed at link time: javac's synthetic `lambda$…`, a
    /// static method reference, a private or `super::` one.
    Java { class_idx: usize, method_idx: usize },
    /// A static method reference to a builtin class (`Integer::parseInt`,
    /// `String::valueOf`): native dispatch on the named class.
    NativeStatic {
        class: &'static str,
        name: &'static str,
        desc: &'static str,
    },
    /// An instance method reference (`String::length`, `Shape::area`,
    /// `s::trim`): resolved on the receiver's runtime class at every call,
    /// exactly as the `invokevirtual` it stands for would be.
    Virtual {
        name: &'static str,
        desc: &'static str,
    },
    /// `Foo::new`: allocate `class`, run its `<init>` on the SAM arguments,
    /// and hand the object back. `init` is `None` for a builtin (`ArrayList::new`),
    /// whose constructor is a native arm.
    Ctor {
        class: &'static str,
        class_bytes: &'static [u8],
        init: Option<(usize, usize)>,
        desc: &'static str,
    },
}

/// Metadata for a lambda proxy object created by `invokedynamic`.
pub struct LambdaProxy {
    pub target: LambdaTarget,
    /// The captured values as the operand-stack slots `invokedynamic`
    /// popped (a captured `long` is two); decoded back into `Value`s when
    /// the body runs.
    pub captures: Vec<Slot>,
    /// The SAM's name. Only a call to this method is the lambda body: a
    /// default method or an `Object` method on the same proxy resolves
    /// through the interface and `Object`, as on any other object.
    pub sam_name: &'static [u8],
}

pub struct ObjectHeap {
    pub(super) objects: ChunkedObjects,
    /// Backing storage for every object's field slots, addressed by
    /// `JvmObject::{fields_off, fields_cap}`. One 8 B [`Slot`] per field
    /// slot: a `long`/`double` field is its low half then its high half,
    /// the JVMS layout `field_slot` numbers by. Freed objects leave dead
    /// spans that [`compact_fields_arena`](Self::compact_fields_arena)
    /// reclaims after each GC sweep (the `ArrayHeap::arena` pattern). Grows
    /// in fixed [`FIELDS_ARENA_CHUNK`] steps — never Vec doubling, which on
    /// a fragmented FreeRTOS heap demands ever-larger contiguous blocks (the
    /// recorded `alloc 90112 bytes failed` incident class).
    pub(super) fields_arena: Vec<Slot>,
    /// Lowest index that might contain a `None` slot; avoids O(n) scans.
    pub(super) first_free: usize,
    /// Canonical `&'static str` per loaded class, indexed by
    /// `JvmObject.class_idx`. Lazily populated on first `alloc` for a class —
    /// real apps load <200 classes, so a linear scan during intern is cheap.
    pub(super) class_table: Vec<&'static str>,
    /// One byte buffer per live StringBuilder, addressed by the slot index the
    /// instance stores in field 0. See [`sb_store`].
    pub(super) sb_bufs: Vec<Option<Vec<u8>>>,
    /// ArrayList / HashMap backing buffers. Elements are always references
    /// (a primitive is boxed before it reaches a collection), so one [`Slot`]
    /// each; the `Value`-typed accessors in `list_store` / `map_store` refuse
    /// a bare `long`/`double`.
    pub(super) list_bufs: Vec<Option<Vec<Slot>>>,
    pub(super) map_bufs: Vec<Option<Vec<(Slot, Slot)>>>,
    /// Sparse list of lambda proxy metadata, keyed by object index.
    pub(super) lambda_proxies: Vec<(u16, LambdaProxy)>,
    /// `Integer.valueOf` & co. must hand back the same object for the range
    /// the JLS (§5.1.7) has every JVM cache — `Integer a = 127, b = 127;
    /// a == b` is `true` on Android. One lazily allocated 256-slot table per
    /// integral wrapper (Integer, Long, Short, Byte, Character); `Boolean`
    /// needs two slots. Entries are GC roots: a shared box never dies.
    pub(super) boxed_cache: [Option<Vec<u16>>; BOX_TABLES],
    pub(super) bool_cache: [u16; 2],
    /// An `OutOfMemoryError` allocated while the heap still had room, thrown
    /// when it no longer has room for one. A root until it is handed out.
    pub(super) oom_reserve: u16,
    /// Sparse list of iterator states, keyed by object index.
    pub(super) iter_states: Vec<(u16, iter_store::IteratorState)>,
    /// Sparse list of `(throwable_obj_idx, string_table_idx)` pairs holding
    /// the message arg passed to `Throwable.<init>(String)` / subclasses.
    pub(super) exception_messages: Vec<(u16, u16)>,
    /// Sparse list of `(throwable_obj_idx, suppressed obj indices)` — the
    /// storage behind `Throwable.addSuppressed`/`getSuppressed`. Entries are
    /// dropped when the owner is swept; the GC mark phase traces the listed
    /// Throwables while the owner is live (after a try-with-resources body
    /// completes they are typically reachable only through this table).
    pub(super) suppressed: Vec<(u16, Vec<u16>)>,
    /// Sparse list of `(throwable_obj_idx, cause_obj_idx)` — the storage
    /// behind `Throwable.getCause()`. Written when the interpreter wraps a
    /// clinit throw in ExceptionInInitializerError. Same GC contract as
    /// `suppressed`: traced on owner mark, dropped on owner sweep.
    pub(super) exception_causes: Vec<(u16, u16)>,
    /// Cumulative allocation count per `class_idx` — the mem-diag "WHO is
    /// churning" histogram. Runtime-gated by [`Self::set_histo_enabled`]
    /// (the sim enables it from `PICODROID_MEMDIAG_HISTO=1`); while disabled
    /// the cost is one branch per alloc and the Vec stays empty. When
    /// enabled its growth (4 B per loaded class) is charged to the heap
    /// like any diagnostic overhead — the histogram is for attribution,
    /// not for byte-exact parity runs.
    #[cfg(feature = "mem-diag")]
    alloc_histo: Vec<u32>,
    #[cfg(feature = "mem-diag")]
    histo_enabled: bool,
    /// Offensive diagnostics: ring of the last 8 span allocations as
    /// `(task_id, offset, n_fields)` — dumped when the span invariant
    /// breaks so the panic names which tasks' allocations interleaved
    /// (task_id via `mem_diag::set_task_id_fn`, 0 when no hook installed).
    #[cfg(feature = "mem-diag")]
    alloc_trace: [(u32, u32, u16); 8],
    #[cfg(feature = "mem-diag")]
    alloc_trace_idx: u8,
    /// Allocations since the interpreter last folded this into GC pacing
    /// (see `Executor::fold_native_alloc_events`).
    alloc_events: u16,
}

impl ObjectHeap {
    pub const fn new() -> Self {
        Self {
            objects: ChunkedObjects::new(),
            fields_arena: Vec::new(),
            first_free: 0,
            class_table: Vec::new(),
            sb_bufs: Vec::new(),
            list_bufs: Vec::new(),
            map_bufs: Vec::new(),
            lambda_proxies: Vec::new(),
            boxed_cache: [None, None, None, None, None],
            bool_cache: [BOX_NONE; 2],
            oom_reserve: BOX_NONE,
            iter_states: Vec::new(),
            exception_messages: Vec::new(),
            suppressed: Vec::new(),
            exception_causes: Vec::new(),
            #[cfg(feature = "mem-diag")]
            alloc_histo: Vec::new(),
            #[cfg(feature = "mem-diag")]
            histo_enabled: false,
            #[cfg(feature = "mem-diag")]
            alloc_trace: [(0, 0, 0); 8],
            #[cfg(feature = "mem-diag")]
            alloc_trace_idx: 0,
            alloc_events: 0,
        }
    }

    /// Drain the pacing counter (see `alloc_events`).
    pub fn take_alloc_events(&mut self) -> u16 {
        core::mem::take(&mut self.alloc_events)
    }

    /// Charge `n` allocations that happened outside this heap on Java's
    /// behalf — native storage a handler grew for a Java object (the JSON
    /// node pool's nodes) — so the GC pacer sees pressure it could not
    /// otherwise observe. Folded in with the heap's own events.
    pub fn charge_alloc_events(&mut self, n: u16) {
        self.alloc_events = self.alloc_events.saturating_add(n);
    }

    /// Enable/disable the per-class allocation histogram (mem-diag). The
    /// platform layer turns this on from a runtime flag; it defaults off so
    /// device mem-diag builds pay one branch per alloc and nothing else.
    #[cfg(feature = "mem-diag")]
    pub fn set_histo_enabled(&mut self, on: bool) {
        self.histo_enabled = on;
    }

    /// The per-class allocation counts, indexed by `class_idx` (empty until
    /// [`Self::set_histo_enabled`]). Resolve names via
    /// [`Self::class_name_by_idx`].
    #[cfg(feature = "mem-diag")]
    pub fn alloc_histo(&self) -> &[u32] {
        &self.alloc_histo
    }

    /// Canonical class name for a `class_idx`, if loaded.
    #[cfg(feature = "mem-diag")]
    pub fn class_name_by_idx(&self, idx: u16) -> Option<&'static str> {
        self.class_table.get(idx as usize).copied()
    }

    /// Associate a message string (StringTable index) with a Throwable object.
    /// Captured by `Throwable.<init>(String, ...)` native dispatchers.
    ///
    /// [`Exhausted`] when the table cannot grow; nothing is recorded then.
    /// The three exception side tables (this one, `suppressed`,
    /// `exception_causes`) double like the lambda registry, and an
    /// infallible push on a full heap is a board reset. Their writers are on
    /// the throw path, so a refusal must not become an `OutOfMemoryError`
    /// that replaces the exception being thrown: callers drop the entry and
    /// throw the original Throwable without its message, cause or
    /// suppressed entry.
    pub fn register_exception_message(
        &mut self,
        obj_idx: u16,
        msg_idx: u16,
    ) -> Result<(), Exhausted> {
        // Replace if an entry exists (e.g. an explicit super("...") chain).
        for entry in self.exception_messages.iter_mut() {
            if entry.0 == obj_idx {
                entry.1 = msg_idx;
                return Ok(());
            }
        }
        reserve_fallible(&mut self.exception_messages, 1)?;
        self.exception_messages.push((obj_idx, msg_idx));
        Ok(())
    }

    /// Look up the message StringTable index for a Throwable object.
    pub fn get_exception_message(&self, obj_idx: u16) -> Option<u16> {
        self.exception_messages
            .iter()
            .find(|(idx, _)| *idx == obj_idx)
            .map(|(_, msg)| *msg)
    }

    /// Drop the message entry for a freed Throwable. Called from GC sweep.
    pub fn free_exception_message(&mut self, obj_idx: u16) {
        self.exception_messages.retain(|(idx, _)| *idx != obj_idx);
    }

    /// Append a suppressed exception to `owner`'s list. Storage behind
    /// `Throwable.addSuppressed` — see the `suppressed` field docs.
    /// [`Exhausted`] when either the owner's list or the table cannot grow;
    /// `owner`'s list is unchanged then (see
    /// [`Self::register_exception_message`] for what callers do with it).
    pub fn add_suppressed(&mut self, owner: u16, throwable: u16) -> Result<(), Exhausted> {
        for (o, list) in self.suppressed.iter_mut() {
            if *o == owner {
                reserve_fallible(list, 1)?;
                list.push(throwable);
                return Ok(());
            }
        }
        reserve_fallible(&mut self.suppressed, 1)?;
        let mut list = Vec::new();
        reserve_fallible(&mut list, 1)?;
        list.push(throwable);
        self.suppressed.push((owner, list));
        Ok(())
    }

    /// The suppressed-exception list recorded for `owner` (empty when none).
    pub fn suppressed_list(&self, owner: u16) -> &[u16] {
        self.suppressed
            .iter()
            .find(|(o, _)| *o == owner)
            .map(|(_, list)| list.as_slice())
            .unwrap_or(&[])
    }

    /// Drop the suppressed list for a freed Throwable. Called from GC sweep.
    pub fn free_suppressed(&mut self, owner: u16) {
        self.suppressed.retain(|(o, _)| *o != owner);
    }

    /// Record `cause` as `owner`'s cause (`Throwable.getCause()`). Replaces
    /// any existing entry, and refuses with [`Exhausted`] when the table
    /// cannot grow, mirroring [`Self::register_exception_message`].
    pub fn register_exception_cause(&mut self, owner: u16, cause: u16) -> Result<(), Exhausted> {
        for entry in self.exception_causes.iter_mut() {
            if entry.0 == owner {
                entry.1 = cause;
                return Ok(());
            }
        }
        reserve_fallible(&mut self.exception_causes, 1)?;
        self.exception_causes.push((owner, cause));
        Ok(())
    }

    /// Look up the cause recorded for `owner`, if any.
    pub fn get_exception_cause(&self, owner: u16) -> Option<u16> {
        self.exception_causes
            .iter()
            .find(|(o, _)| *o == owner)
            .map(|(_, c)| *c)
    }

    /// Drop the cause entry for a freed Throwable. Called from GC sweep.
    pub fn free_exception_cause(&mut self, owner: u16) {
        self.exception_causes.retain(|(o, _)| *o != owner);
    }
}

impl Default for ObjectHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectHeap {
    /// Allocate a new object of the given class, returning its heap index.
    /// Reuses a None slot (freed by GC) before growing the backing Vec.
    pub fn alloc(&mut self, class_name: &'static str) -> Option<u16> {
        self.alloc_with_field_count(class_name, default_field_count_for_native(class_name))
    }

    /// Like [`alloc`], but reserves storage for `n_fields` field *slots* up
    /// front (a `long`/`double` field is two) so callers that know the
    /// layout (native handlers, `op_new` via [`alloc_with_defaults`]) skip
    /// the lazy-grow path inside [`set_field`]. Behaviour is otherwise
    /// identical to [`alloc`].
    pub fn alloc_with_field_count(
        &mut self,
        class_name: &'static str,
        n_fields: usize,
    ) -> Option<u16> {
        self.alloc_events = self.alloc_events.saturating_add(1);
        let class_idx = self.intern_class(class_name)?;
        // Span reservation + descriptor write must be scheduler-atomic: an
        // equal-priority wake yield inside the arena resize let two tasks
        // read the same arena length, and the loser's resize truncated the
        // winner's fresh span (see `atomic_section` module docs).
        let _atomic = crate::atomic_section::AtomicSection::enter();
        let fields_off = self.alloc_span(n_fields)?;
        let Some(idx) = self.place_in_slot(JvmObject {
            fields_off,
            class_idx,
            field_count: 0,
            fields_cap: n_fields as u8,
        }) else {
            // The span is at the arena tail and nothing has read it, so give
            // it back rather than leak it into every later GC's working set.
            self.fields_arena.truncate(fields_off as usize);
            return None;
        };
        #[cfg(feature = "mem-diag")]
        self.debug_check_spans("post-alloc");
        Some(idx)
    }

    /// Fixed growth step for [`fields_arena`](Self::fields_arena), in
    /// [`Slot`]s (256 × 8 B = 2 KB per step). Chunked growth keeps the
    /// arena's contiguous-block demands on the device allocator bounded and
    /// constant, per the chunked_slots.rs precedent.
    const FIELDS_ARENA_CHUNK: usize = 256;

    /// Reserve a zero-initialised `n_fields`-slot span at the arena tail.
    /// Returns its offset, or `None` on allocation failure (caller triggers
    /// GC and retries, like every other JVM allocation) or when `n_fields`
    /// exceeds the u8 capacity a descriptor can record.
    fn alloc_span(&mut self, n_fields: usize) -> Option<u32> {
        if n_fields == 0 {
            return Some(0);
        }
        if n_fields > u8::MAX as usize {
            return None;
        }
        let need = self.fields_arena.len() + n_fields;
        if need > self.fields_arena.capacity() {
            let short = need - self.fields_arena.capacity();
            // Manual div_ceil: the crate's MSRV (1.70) predates usize::div_ceil.
            let grow = (short + Self::FIELDS_ARENA_CHUNK - 1) / Self::FIELDS_ARENA_CHUNK
                * Self::FIELDS_ARENA_CHUNK;
            let additional = self.fields_arena.capacity() + grow - self.fields_arena.len();
            if self.fields_arena.try_reserve_exact(additional).is_err() {
                return None;
            }
        }
        let off = self.fields_arena.len() as u32;
        self.fields_arena.resize(need, Slot::Null);
        #[cfg(feature = "mem-diag")]
        {
            let i = self.alloc_trace_idx as usize % self.alloc_trace.len();
            self.alloc_trace[i] = (crate::mem_diag::task_id(), off, n_fields as u16);
            self.alloc_trace_idx = self.alloc_trace_idx.wrapping_add(1);
        }
        Some(off)
    }

    /// Look up `name` in [`class_table`] (linear scan), inserting it if
    /// missing. Returns the resulting index. Class-name pointers are
    /// canonical (see [`crate::interpreter::helpers::class_name_to_static_in`])
    /// so byte-equality on `&'static str` is fast.
    /// `None` when the table cannot grow — the same allocation failure as
    /// the object's own slot, reported rather than aborted.
    fn intern_class(&mut self, name: &'static str) -> Option<u16> {
        for (i, &existing) in self.class_table.iter().enumerate() {
            if core::ptr::eq(existing.as_ptr(), name.as_ptr()) && existing.len() == name.len()
                || crate::class_file::name_eq(existing.as_bytes(), name.as_bytes())
            {
                return Some(i as u16);
            }
        }
        let idx = self.class_table.len() as u16;
        reserve_fallible(&mut self.class_table, 1).ok()?;
        self.class_table.push(name);
        Some(idx)
    }

    /// Find a free slot (reusing GC-freed None entries before growing) and
    /// place `obj` in it. Returns the slot index, or `None` when the slot
    /// table cannot grow — the allocation failure every caller of
    /// [`alloc`](Self::alloc) already handles by collecting and retrying.
    /// Growing it used to be an infallible `push`, so an exhausted heap
    /// aborted the firmware here — a board reset from the most
    /// Java-reachable allocation there is (found while testing the QA
    /// 2026-09-13 out-of-memory fixes; `intern_dyn` got the same treatment
    /// in `e3c5f008`).
    fn place_in_slot(&mut self, obj: JvmObject) -> Option<u16> {
        #[cfg(feature = "mem-diag")]
        if self.histo_enabled {
            let ci = obj.class_idx as usize;
            if self.alloc_histo.len() <= ci {
                self.alloc_histo.resize(ci + 1, 0);
            }
            self.alloc_histo[ci] = self.alloc_histo[ci].wrapping_add(1);
        }
        while self.first_free < self.objects.len() {
            if self.objects[self.first_free].is_none() {
                let idx = self.first_free;
                self.objects[idx] = Some(obj);
                self.first_free = idx + 1;
                return Some(idx as u16);
            }
            self.first_free += 1;
        }
        let idx = self.objects.len() as u16;
        self.objects.try_push(Some(obj))?;
        self.first_free = self.objects.len();
        Some(idx)
    }

    /// Allocate and initialize every declared instance field to its JVMS §2.3
    /// typed default (0 for integral, 0.0 for fp, `Null` for reference).
    /// Walks the superclass chain root-to-leaf, matching the slot layout used
    /// by `interpreter::helpers::field_slot`.  Callers without class metadata
    /// should keep using [`alloc`].
    pub fn alloc_with_defaults(
        &mut self,
        class_name: &'static str,
        classes: &[ClassFile],
    ) -> Option<u16> {
        // Build chain root-first, tracking whether the chain bottoms out at
        // java/lang/Enum (a native class outside `classes` with 2 implicit
        // reference-typed fields — those stay Null, matching field_slot).
        let mut chain: Vec<usize> = Vec::new();
        let mut enum_base = false;
        let mut current: &str = class_name;
        // Canonical, genuinely-`'static` (Flash-backed) name for the leaf class.
        // `class_name` may be a transient pointer — e.g. a native caller can
        // resolve an Intent's target-class name to a GC-managed dynamic String
        // and transmute it to `&'static` — and interning that into `class_table`
        // leaves a dangling entry once the dynamic String is swept. Adopting the
        // loaded class file's own name keeps `class_table` pointing at Flash for
        // the JVM's lifetime. Falls back to `class_name` for classes not present
        // in `classes` (builtins/native), whose names are already `'static`.
        let mut canonical_name: &'static str = class_name;
        loop {
            let ci = crate::class_file::find_class(classes, current.as_bytes());
            match ci {
                Some(i) => {
                    if chain.is_empty() {
                        if let Some(n) = classes[i].class_name() {
                            if let Ok(s) = core::str::from_utf8(n) {
                                canonical_name = s;
                            }
                        }
                    }
                    chain.push(i);
                    match classes[i].super_class_name() {
                        None => break,
                        Some(super_bytes) => match core::str::from_utf8(super_bytes) {
                            Ok(s) => current = s,
                            Err(_) => break,
                        },
                    }
                }
                None => {
                    if current == c::java_lang_Enum {
                        enum_base = true;
                    }
                    break;
                }
            }
        }
        chain.reverse();

        // Size the backing span exactly once, before allocating, so the
        // default-writing loop below never triggers a reallocation. Slots,
        // not fields: a `long`/`double` field takes two.
        let n_fields = (if enum_base { ENUM_IMPLICIT_FIELDS } else { 0 })
            + chain
                .iter()
                .map(|&ci| {
                    let cf = &classes[ci];
                    cf.fields()
                        .iter()
                        .map(|fi| {
                            cf.field_descriptor(fi)
                                .map_or(1, Value::descriptor_slot_width)
                        })
                        .sum::<usize>()
                })
                .sum::<usize>();
        let idx = self.alloc_with_field_count(canonical_name, n_fields)?;

        // Write the defaults straight into the arena rather than through
        // `set_field` once per field.
        //
        // `set_field` exists to be safe for an object other tasks can already
        // see, so it pays for a slot re-lookup, the lazy-grow branch, a
        // high-water bump, and — the expensive part — a scheduler-atomic
        // section on *every* field. On device that is a
        // vTaskSuspendAll/xTaskResumeAll pair per field written, so a
        // ten-field object suspended and resumed the scheduler ten times just
        // to store ten constants.
        //
        // None of that is needed here. `idx` has not been published: it is
        // still local to this call, so no other task can read or write these
        // slots and there is no torn-`Value` hazard to guard against. The
        // capacity was sized exactly above, so the lazy-grow path is
        // unreachable. One section around the whole run is all that is
        // required, and it is required only because another task could be
        // reallocating `fields_arena` underneath us.
        let start_slot = if enum_base { ENUM_IMPLICIT_FIELDS } else { 0 };
        {
            let _atomic = crate::atomic_section::AtomicSection::enter();
            let base = self.objects.get(idx as usize)?.as_ref()?.fields_off as usize;
            let mut slot = start_slot;
            for ci in chain.iter() {
                let cf = &classes[*ci];
                for fi in cf.fields() {
                    let v = match cf.field_descriptor(fi) {
                        Some(desc) => default_for_descriptor(desc),
                        None => Value::Null,
                    };
                    let (lo, hi) = v.to_slots();
                    self.fields_arena[base + slot] = lo;
                    slot += 1;
                    if let Some(hi) = hi {
                        self.fields_arena[base + slot] = hi;
                        slot += 1;
                    }
                }
            }
            // Match `set_field`'s high-water semantics exactly: it raises
            // `field_count` to the highest slot actually written, and leaves
            // it alone when nothing is written at all. An enum whose chain
            // contributes no fields must keep `field_count == 0` so that
            // reads of the two implicit slots still return None.
            if slot > start_slot {
                let s = self.objects.get_mut(idx as usize)?.as_mut()?;
                if slot > s.field_count as usize {
                    s.field_count = slot as u8;
                }
            }
        }
        Some(idx)
    }

    /// Shallow-copy `idx` per `Object.clone()`: a new object of the same
    /// class whose field slots are copied verbatim — reference-typed fields
    /// share their referents (Java-spec shallow semantics). The high-water
    /// `field_count` carries over, so uninitialised-slot reads behave like
    /// the original's. Returns `None` for an invalid or GC-freed index.
    pub fn clone_object(&mut self, idx: u16) -> Option<u16> {
        let src = *self.objects.get(idx as usize)?.as_ref()?;
        let new_off = self.alloc_span(src.fields_cap as usize)?;
        if src.fields_cap > 0 {
            let from = src.fields_off as usize;
            self.fields_arena
                .copy_within(from..from + src.fields_cap as usize, new_off as usize);
        }
        let placed = self.place_in_slot(JvmObject {
            fields_off: new_off,
            ..src
        });
        if placed.is_none() {
            self.fields_arena.truncate(new_off as usize);
        }
        placed
    }

    /// Read the field starting at slot `field`. A `long`/`double` field is
    /// read from its two slots; `None` for an unset slot, an out-of-range
    /// index, or the second slot of a category-2 field addressed on its own
    /// (a native table numbering fields by one slot each — the
    /// `native_field_tables_match_the_class_files` test in `picodroid-core`
    /// catches those).
    pub fn get_field(&self, idx: u16, field: usize) -> Option<Value> {
        let obj = self.objects.get(idx as usize)?.as_ref()?;
        let count = obj.field_count as usize;
        if field >= count {
            return None;
        }
        let base = obj.fields_off as usize;
        let lo = *self.fields_arena.get(base + field)?;
        if let Some(v) = lo.to_value() {
            return Some(v);
        }
        if field + 1 >= count {
            return None;
        }
        Slot::assemble(lo, *self.fields_arena.get(base + field + 1)?)
    }

    /// Write `v` at slot `field` — two slots for a `long`/`double`, inside
    /// the one scheduler-atomic section, so no other task ever sees one
    /// half of a category-2 field updated (a torn read of a non-volatile
    /// `long` is allowed by the JLS, but the writer side stays whole).
    pub fn set_field(&mut self, idx: u16, field: usize, v: Value) -> Option<()> {
        // Atomic for the lazy-grow path (span move + descriptor update) —
        // same interleave hazard as alloc_with_field_count.
        let _atomic = crate::atomic_section::AtomicSection::enter();
        let obj = *self.objects.get(idx as usize)?.as_ref()?;
        let (lo, hi) = v.to_slots();
        let needed = field + 1 + hi.is_some() as usize;
        if needed > obj.fields_cap as usize {
            // Lazy grow — a caller wrote past the count it declared at alloc
            // time (rare; native handlers should pass the right `n_fields`).
            // Move the span to a fresh tail allocation; the old span becomes
            // garbage until the next GC arena compaction.
            let new_cap = needed;
            let new_off = self.alloc_span(new_cap)?;
            if obj.fields_cap > 0 {
                let from = obj.fields_off as usize;
                self.fields_arena
                    .copy_within(from..from + obj.fields_cap as usize, new_off as usize);
            }
            let slot = self.objects.get_mut(idx as usize)?.as_mut()?;
            slot.fields_off = new_off;
            slot.fields_cap = new_cap as u8;
            #[cfg(feature = "mem-diag")]
            self.debug_check_spans("post-lazy-grow");
        }
        let slot = self.objects.get_mut(idx as usize)?.as_mut()?;
        if needed > slot.field_count as usize {
            slot.field_count = needed as u8;
        }
        let at = slot.fields_off as usize + field;
        self.fields_arena[at] = lo;
        if let Some(hi) = hi {
            self.fields_arena[at + 1] = hi;
        }
        Some(())
    }

    pub fn class_name(&self, idx: u16) -> Option<&'static str> {
        let class_idx = self.objects.get(idx as usize)?.as_ref()?.class_idx;
        self.class_table.get(class_idx as usize).copied()
    }

    // ── GC support ───────────────────────────────────────────────────────────

    /// Total number of slots (including freed `None` slots).
    /// Allocated slot chunks (diagnostics / pre-reservation sizing).
    pub fn slot_chunk_count(&self) -> usize {
        self.objects.chunk_count()
    }

    /// Current fields-arena capacity in [`Slot`]s (8 B each).
    pub fn fields_arena_capacity(&self) -> usize {
        self.fields_arena.capacity()
    }

    /// Boot-time pre-reservation: claim slot chunks and fields-arena
    /// capacity while the heap is young and contiguous, so steady-state
    /// storage doesn't land mid-heap during Activity churn and strand the
    /// free space around it (PEM-3). Best-effort; a failed reservation just
    /// leaves on-demand growth in place.
    pub fn prereserve(&mut self, slot_chunks: usize, fields_values: usize) {
        self.objects.reserve_chunks(slot_chunks);
        let target = fields_values.saturating_sub(self.fields_arena.len());
        if self.fields_arena.capacity() < fields_values {
            let _ = self.fields_arena.try_reserve_exact(target);
        }
    }

    pub fn slot_count(&self) -> usize {
        self.objects.len()
    }

    /// Returns `true` if the slot at `idx` contains a live object.
    pub fn is_live(&self, idx: u16) -> bool {
        self.objects.get(idx as usize).is_some_and(|o| o.is_some())
    }

    /// Free the object at `idx`, setting its slot to `None`.
    pub fn free(&mut self, idx: u16) {
        let i = idx as usize;
        if let Some(slot) = self.objects.get_mut(i) {
            // Offensive mode: poison the whole fields-arena span before it
            // becomes garbage-awaiting-compaction. Any code that still reads
            // through a stale span sees the pattern instead of plausible
            // stale Values, and the GC mark phase panics if poison ever
            // appears inside a LIVE object's fields (arena-compaction bug /
            // use-after-free).
            #[cfg(feature = "mem-diag")]
            if crate::mem_diag::offensive() {
                if let Some(obj) = slot.as_ref() {
                    let start = obj.fields_off as usize;
                    let end = start + obj.fields_cap as usize;
                    if end <= self.fields_arena.len() {
                        for v in &mut self.fields_arena[start..end] {
                            *v = Slot::Int(crate::mem_diag::POISON_I32);
                        }
                    }
                }
            }
            *slot = None;
            if i < self.first_free {
                self.first_free = i;
            }
        }
    }

    /// Structural integrity sweep (mem-diag offensive mode): every live
    /// object's arena span is in-bounds and non-overlapping, `first_free`
    /// never skips a free slot, and the chunked slot store is consistent.
    #[cfg(feature = "mem-diag")]
    pub fn integrity_check(&self) -> Result<(), &'static str> {
        if !self.objects.invariant_holds() {
            return Err("ObjectHeap: ChunkedSlots chunk/len invariant broken");
        }
        for i in 0..self.first_free.min(self.objects.len()) {
            if self.objects[i].is_none() {
                return Err("ObjectHeap: free slot below first_free");
            }
        }
        let arena_len = self.fields_arena.len();
        for (i, slot) in self.objects.iter().enumerate() {
            let Some(a) = slot.as_ref() else { continue };
            if a.field_count > a.fields_cap {
                return Err("ObjectHeap: field_count exceeds fields_cap");
            }
            let (a_start, a_len) = (a.fields_off as usize, a.fields_cap as usize);
            if a_start + a_len > arena_len {
                return Err("ObjectHeap: fields span out of arena bounds");
            }
            if a_len == 0 {
                continue;
            }
            for slot_b in self.objects.iter().skip(i + 1) {
                let Some(b) = slot_b.as_ref() else { continue };
                let (b_start, b_len) = (b.fields_off as usize, b.fields_cap as usize);
                if b_len == 0 {
                    continue;
                }
                if a_start < b_start + b_len && b_start < a_start + a_len {
                    return Err("ObjectHeap: overlapping fields-arena spans");
                }
            }
        }
        Ok(())
    }

    /// Return the slice of populated field slots for the object at `idx`,
    /// used by the GC tracer.  Empty when the slot is freed or out of bounds.
    pub fn fields_slice(&self, idx: u16) -> &[Slot] {
        match self.objects.get(idx as usize).and_then(|o| o.as_ref()) {
            Some(o) => {
                let start = o.fields_off as usize;
                &self.fields_arena[start..start + o.field_count as usize]
            }
            None => &[],
        }
    }

    /// Compact the fields arena by sliding live spans down over the garbage
    /// left by swept objects and lazy-grow moves. Called by GC after sweep;
    /// mirrors [`crate::array_heap::ArrayHeap::compact_arena`] and shares
    /// its scratch buffer. A heap too full to hold the scratch buffer skips
    /// the compaction (the dead spans wait for a later cycle) rather than
    /// aborting inside the collector.
    pub fn compact_fields_arena(&mut self, buf: &mut Vec<u64>) {
        buf.clear();
        let live = self
            .objects
            .iter()
            .filter(|s| s.as_ref().is_some_and(|o| o.fields_cap > 0))
            .count();
        if buf.try_reserve(live).is_err() {
            return;
        }
        for (i, slot) in self.objects.iter().enumerate() {
            if let Some(obj) = slot.as_ref() {
                if obj.fields_cap > 0 {
                    // Slots are addressed by `ObjectRef(u16)`, so an index
                    // always fits the 16 bits reserved for it in the key.
                    debug_assert!(
                        i <= u16::MAX as usize,
                        "object slot index overflows the key"
                    );
                    buf.push(
                        ((obj.fields_off as u64) << 32)
                            | ((i as u64) << 16)
                            | obj.fields_cap as u64,
                    );
                }
            }
        }
        crate::sort::sort_keys(buf);

        let mut write_pos: usize = 0;
        for &key in buf.iter() {
            let (slot_idx, read_offset, cap) = (
                (key >> 16) as usize & 0xffff,
                (key >> 32) as u32,
                key as u16,
            );
            let read_pos = read_offset as usize;
            let count = cap as usize;
            if read_pos != write_pos {
                self.fields_arena
                    .copy_within(read_pos..read_pos + count, write_pos);
            }
            if let Some(Some(obj)) = self.objects.get_mut(slot_idx) {
                obj.fields_off = write_pos as u32;
            }
            write_pos += count;
        }
        self.fields_arena.truncate(write_pos);
        #[cfg(feature = "mem-diag")]
        self.debug_check_spans("post-compact");
    }

    /// Offensive invariant: every live object's field span lies inside the
    /// arena. Sequential execution preserves this by construction (spans are
    /// only created at the tail and only compaction truncates), so a firing
    /// here proves the alloc/compact sequence was interleaved by another
    /// context — panics at the moment the inconsistency becomes visible,
    /// naming the call site. Added for the picoenvmon compaction panic
    /// (`range end index N out of range`, docs/picoenvmon-qa.md 2026-08-17).
    #[cfg(feature = "mem-diag")]
    pub fn debug_check_spans(&self, ctx: &str) {
        if !crate::mem_diag::offensive() {
            return;
        }
        let len = self.fields_arena.len();
        for (i, slot) in self.objects.iter().enumerate() {
            if let Some(o) = slot.as_ref() {
                let end = o.fields_off as usize + o.fields_cap as usize;
                if end > len {
                    panic!(
                        "mem-diag: span invariant broken at {}: obj {} span {}..{} > arena len {} — alloc trace {:?}",
                        ctx, i, o.fields_off, end, len, self.alloc_trace
                    );
                }
            }
        }
        // Bounds alone miss the interleaved-alloc signature (two tasks read
        // the same arena len -> duplicate offsets, overlapping spans, each
        // individually in-bounds). The overlap sweep in integrity_check
        // catches that shape; the alloc trace names the interleaved tasks.
        if let Err(m) = self.integrity_check() {
            panic!(
                "mem-diag: heap integrity broken at {}: {} — alloc trace {:?}",
                ctx, m, self.alloc_trace
            );
        }
    }

    /// Approximate bytes held live by this heap. Used by `perfbench` /
    /// `Runtime.usedMemory()` to track heap pressure across optimisation
    /// changes. Sums the per-object slot size plus the arena span capacity
    /// (not high-water `field_count`, so we report what's actually pinned).
    ///
    /// Since M6 the descriptor is pointer-free, so host and device sizes are
    /// identical by construction — `size_of` IS the device figure
    /// (docs/parity-audit.md OBJ-01/M6).
    pub fn live_bytes(&self) -> usize {
        const PER_OBJECT: usize = core::mem::size_of::<Option<JvmObject>>();
        const _: () = assert!(PER_OBJECT == 12); // identical on all targets (M6)
        const PER_FIELD: usize = core::mem::size_of::<Slot>();
        const _: () = assert!(PER_FIELD == 8); // identical on all targets (V1)
        let mut total = 0;
        for i in 0..self.objects.len() {
            if let Some(Some(obj)) = self.objects.get(i) {
                total += PER_OBJECT + obj.fields_cap as usize * PER_FIELD;
            }
        }
        total
    }

    /// Number of classes in the intern table — sizes a
    /// [`Self::census_by_class`] output buffer.
    #[cfg(feature = "mem-diag")]
    pub fn class_count(&self) -> usize {
        self.class_table.len()
    }

    /// Live-set census bucketed by class: `out[class_idx]` accumulates the
    /// count and pinned bytes (slot + field span, the [`Self::live_bytes`]
    /// accounting) of every live object of that class. Unlike the alloc
    /// histogram — cumulative churn since boot — this is a snapshot of what
    /// is retained right now. Caller supplies a zeroed `out` sized from
    /// [`Self::class_count`]; the heap side never allocates (monitor rule).
    #[cfg(feature = "mem-diag")]
    pub fn census_by_class(&self, out: &mut [ClassCensus]) {
        const PER_OBJECT: u32 = core::mem::size_of::<Option<JvmObject>>() as u32;
        const PER_FIELD: u32 = core::mem::size_of::<Slot>() as u32;
        for i in 0..self.objects.len() {
            if let Some(Some(obj)) = self.objects.get(i) {
                if let Some(row) = out.get_mut(obj.class_idx as usize) {
                    row.count += 1;
                    row.bytes += PER_OBJECT + obj.fields_cap as u32 * PER_FIELD;
                }
            }
        }
    }

    /// Bytes pinned by the ObjectHeap's side tables — storage that
    /// [`Self::live_bytes`] does NOT count: ArrayList/HashMap backing
    /// buffers, the StringBuilder text stack, lambda captures, and the
    /// exception tables. Payload bytes only (element size × capacity);
    /// the containers' own Vec headers are pointer-width-dependent and
    /// excluded so the figure reads the same on host and device.
    #[cfg(feature = "mem-diag")]
    pub fn side_table_census(&self) -> SideTableCensus {
        const PER_VALUE: u32 = core::mem::size_of::<Slot>() as u32;
        let mut c = SideTableCensus::default();
        for buf in self.list_bufs.iter().flatten() {
            c.list_count += 1;
            c.list_bytes += buf.capacity() as u32 * PER_VALUE;
        }
        for buf in self.map_bufs.iter().flatten() {
            c.map_count += 1;
            c.map_bytes += buf.capacity() as u32 * 2 * PER_VALUE;
        }
        for buf in self.sb_bufs.iter().flatten() {
            c.sb_bytes += buf.capacity() as u32;
        }
        for (_, proxy) in &self.lambda_proxies {
            c.lambda_count += 1;
            c.lambda_bytes += proxy.captures.capacity() as u32 * PER_VALUE;
        }
        // Exception tables: entry pairs are 4 B each; suppressed lists add
        // their inner u16 capacity.
        c.exc_bytes += (self.exception_messages.len() as u32 + self.exception_causes.len() as u32)
            * 2
            * core::mem::size_of::<u16>() as u32;
        for (_, list) in &self.suppressed {
            c.exc_bytes += 2 * core::mem::size_of::<u16>() as u32
                + list.capacity() as u32 * core::mem::size_of::<u16>() as u32;
        }
        c
    }
}

/// One row of the live-heap census: live-object count and pinned bytes for a
/// single `class_idx` (see [`ObjectHeap::census_by_class`]).
#[cfg(feature = "mem-diag")]
#[derive(Clone, Copy, Default)]
pub struct ClassCensus {
    pub count: u32,
    pub bytes: u32,
}

/// Byte totals for the ObjectHeap side tables that `live_bytes` misses (see
/// [`ObjectHeap::side_table_census`]).
#[cfg(feature = "mem-diag")]
#[derive(Clone, Copy, Default)]
pub struct SideTableCensus {
    pub list_count: u32,
    pub list_bytes: u32,
    pub map_count: u32,
    pub map_bytes: u32,
    pub sb_bytes: u32,
    pub lambda_count: u32,
    pub lambda_bytes: u32,
    pub exc_bytes: u32,
}

mod num_fmt;
pub use self::num_fmt::*;

#[cfg(test)]
mod tests;

// ── Fallible growth of the side buffers ─────────────────────────────────

/// A side buffer (list, map, builder) could not grow: the Rust heap has no
/// block of the needed size. Native arms turn this into a Java
/// `OutOfMemoryError`; the alternative — `Vec::push` aborting through the
/// allocation-error handler — resets the board (QA 2026-09-13: a 2000-entry
/// `HashMap` did exactly that in the sim).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exhausted;

/// Make room for `extra` more elements in `v` without aborting. `Vec`'s own
/// growth doubles, which on a fragmented FreeRTOS arena asks for a block
/// twice the current one; when that fails, a quarter step (or the exact
/// need) is tried before giving up, since the arena may still hold a
/// smaller block.
pub(crate) fn reserve_fallible<T>(v: &mut Vec<T>, extra: usize) -> Result<(), Exhausted> {
    if v.capacity() - v.len() >= extra {
        return Ok(());
    }
    if v.try_reserve(extra).is_ok() {
        return Ok(());
    }
    // Doubling asks a fragmented arena for a block it may not have: step
    // down by halves to the exact need before giving up.
    let mut step = v.len() / 2;
    while step > extra {
        if v.try_reserve_exact(step).is_ok() {
            return Ok(());
        }
        step /= 2;
    }
    v.try_reserve_exact(extra).map_err(|_| Exhausted)
}

/// Key equality for the builtin collections, as `Object.equals` has it for
/// the classes without a body of their own: strings by content, boxes by
/// class and value — `Integer(1)` is not `Short(1)`, and `Double`/`Float`
/// follow `Double.equals` (NaN equals NaN, `0.0` is not `-0.0`) — and every
/// other object by identity. A class with its own `equals(Object)` is
/// served by the interpreter's equals-aware path instead. Until QA
/// 2026-09-13 any two objects with an equal field 0 were one key: the
/// boxes of different classes above, and two unrelated objects alike.
pub(crate) fn key_eq(
    a: Value,
    b: Value,
    objects: &ObjectHeap,
    strings: &crate::heap::StringTable,
) -> bool {
    match (a, b) {
        (Value::ObjectRef(ai), Value::ObjectRef(bi)) if ai != bi => {
            let (Some(ca), Some(cb)) = (objects.class_name(ai), objects.class_name(bi)) else {
                return false;
            };
            if ca != cb || !is_box_class(ca) {
                return false;
            }
            match (objects.get_field(ai, 0), objects.get_field(bi, 0)) {
                (Some(Value::Double(x)), Some(Value::Double(y))) => {
                    x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
                }
                (Some(Value::Float(x)), Some(Value::Float(y))) => {
                    x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
                }
                (Some(x), Some(y)) => x == y,
                _ => false,
            }
        }
        // Distinct String References can carry the same text (a literal
        // vs. a runtime-built string).
        (Value::Reference(ai), Value::Reference(bi)) => strings.content_eq(ai, bi),
        _ => a == b,
    }
}

fn is_box_class(class: &str) -> bool {
    matches!(
        class,
        c::java_lang_Integer
            | c::java_lang_Long
            | c::java_lang_Short
            | c::java_lang_Byte
            | c::java_lang_Character
            | c::java_lang_Boolean
            | c::java_lang_Float
            | c::java_lang_Double
    )
}

mod boxed_cache;
#[allow(unused_imports)]
use self::boxed_cache::*;

#[cfg(test)]
mod growth_tests;
