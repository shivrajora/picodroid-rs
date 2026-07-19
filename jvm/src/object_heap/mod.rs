// SPDX-License-Identifier: GPL-3.0-only
pub(crate) mod iter_store;
mod lambda;
mod list_store;
mod map_store;

use crate::chunked_slots::ChunkedSlots;
use crate::class_file::ClassFile;
use crate::types::{default_for_descriptor, Value};
use alloc::boxed::Box;
use alloc::vec::Vec;

/// Chunked-slot storage for `Option<JvmObject>`. See [`crate::chunked_slots`].
type ChunkedObjects = ChunkedSlots<JvmObject>;

/// Number of implicit fields in `java/lang/Enum` (name + ordinal).
const ENUM_IMPLICIT_FIELDS: usize = 2;

/// Pre-allocation hint for [`ObjectHeap::alloc`].  Native handlers that create
/// view / wrapper objects know exactly how many fields they will write — feed
/// that count back to the heap so the backing slice is sized once instead of
/// reallocating inside [`JvmObject::set_field`].  Unlisted classes default to
/// 0; the slice still grows lazily, just at the cost of one extra reallocation.
fn default_field_count_for_native(class_name: &str) -> usize {
    match class_name {
        // Boxed wrappers store the unboxed value at slot 0.
        "java/lang/Integer"
        | "java/lang/Boolean"
        | "java/lang/Long"
        | "java/lang/Float"
        | "java/lang/Double"
        | "java/lang/Character"
        | "java/lang/Byte"
        | "java/lang/Short" => 1,
        // HashMap views store the backing map_buf index at slot 0.
        "java/util/HashMap$KeySet" | "java/util/HashMap$Values" => 1,
        _ => 0,
    }
}

#[derive(Clone)]
pub struct JvmObject {
    /// Index into [`ObjectHeap::class_table`]. Resolves back to the canonical
    /// `&'static str` class name via [`ObjectHeap::class_name`]; storing only
    /// the index here saves 14 B per object versus a direct `&'static str`.
    class_idx: u16,
    /// High-water mark of explicit `set_field` writes — preserves the JVMS
    /// "uninitialised slot reads as None" contract that the
    /// `alloc_without_defaults_still_leaves_slots_unset` regression test
    /// in `interpreter/tests/fields.rs` enforces. May be less than `fields.len()`.
    field_count: u8,
    /// Field storage, sized once at allocation. Lazily reallocated by
    /// [`set_field`] if a caller writes past the initial capacity — should be
    /// rare in production code (every native handler should pass the correct
    /// `n_fields` to [`ObjectHeap::alloc_with_field_count`]).
    fields: Box<[Value]>,
}

// Compile-time guard against future regressions.  Niche-packed
// `Option<JvmObject>` sits at 24 B — `Box<[Value]>`'s non-null pointer absorbs
// the discriminant, so the option tag is free.  Loosen the bound only with
// supporting measurement.
const _: () = assert!(core::mem::size_of::<Option<JvmObject>>() <= 24);

impl JvmObject {
    fn new_with_field_count(class_idx: u16, n_fields: usize) -> Self {
        let fields = if n_fields == 0 {
            Box::<[Value]>::default()
        } else {
            alloc::vec![Value::Null; n_fields].into_boxed_slice()
        };
        Self {
            class_idx,
            field_count: 0,
            fields,
        }
    }

    fn get_field(&self, field: usize) -> Option<Value> {
        if field >= self.field_count as usize {
            return None;
        }
        self.fields.get(field).copied()
    }

    fn set_field(&mut self, field: usize, v: Value) {
        if field >= self.fields.len() {
            // Lazy grow — kicks in when a caller writes past the count it
            // declared at alloc time.  Reallocates the boxed slice; rare in
            // production (debug builds can flag this via a heap counter).
            let mut buf: Vec<Value> = self.fields.to_vec();
            buf.resize(field + 1, Value::Null);
            self.fields = buf.into_boxed_slice();
        }
        let needed = field + 1;
        if needed > self.field_count as usize {
            self.field_count = needed as u8;
        }
        self.fields[field] = v;
    }

    /// Returns the slice of populated fields (`field_count` long), used by
    /// the GC tracer to walk an object's references.
    fn fields_slice(&self) -> &[Value] {
        &self.fields[..self.field_count as usize]
    }
}

/// Metadata for a lambda proxy object created by `invokedynamic`.
pub struct LambdaProxy {
    pub target_class_idx: usize,
    pub target_method_idx: usize,
    pub captures: Vec<Value>,
}

pub struct ObjectHeap {
    pub(super) objects: ChunkedObjects,
    /// Lowest index that might contain a `None` slot; avoids O(n) scans.
    pub(super) first_free: usize,
    /// Canonical `&'static str` per loaded class, indexed by
    /// `JvmObject.class_idx`. Lazily populated on first `alloc` for a class —
    /// real apps load <200 classes, so a linear scan during intern is cheap.
    pub(super) class_table: Vec<&'static str>,
    pub(super) sb_stack: Vec<Vec<u8>>,
    pub(super) list_bufs: Vec<Option<Vec<Value>>>,
    pub(super) map_bufs: Vec<Option<Vec<(Value, Value)>>>,
    /// Sparse list of lambda proxy metadata, keyed by object index.
    pub(super) lambda_proxies: Vec<(u16, LambdaProxy)>,
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
}

impl ObjectHeap {
    pub const fn new() -> Self {
        Self {
            objects: ChunkedObjects::new(),
            first_free: 0,
            class_table: Vec::new(),
            sb_stack: Vec::new(),
            list_bufs: Vec::new(),
            map_bufs: Vec::new(),
            lambda_proxies: Vec::new(),
            iter_states: Vec::new(),
            exception_messages: Vec::new(),
            suppressed: Vec::new(),
            exception_causes: Vec::new(),
        }
    }

    /// Associate a message string (StringTable index) with a Throwable object.
    /// Captured by `Throwable.<init>(String, ...)` native dispatchers.
    pub fn register_exception_message(&mut self, obj_idx: u16, msg_idx: u16) {
        // Replace if an entry exists (e.g. an explicit super("...") chain).
        for entry in self.exception_messages.iter_mut() {
            if entry.0 == obj_idx {
                entry.1 = msg_idx;
                return;
            }
        }
        self.exception_messages.push((obj_idx, msg_idx));
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
    pub fn add_suppressed(&mut self, owner: u16, throwable: u16) {
        for (o, list) in self.suppressed.iter_mut() {
            if *o == owner {
                list.push(throwable);
                return;
            }
        }
        self.suppressed.push((owner, alloc::vec![throwable]));
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
    /// any existing entry, mirroring `register_exception_message`.
    pub fn register_exception_cause(&mut self, owner: u16, cause: u16) {
        for entry in self.exception_causes.iter_mut() {
            if entry.0 == owner {
                entry.1 = cause;
                return;
            }
        }
        self.exception_causes.push((owner, cause));
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
    /// For `java/lang/StringBuilder`, reuses an existing slot if one exists —
    /// the JVM's single shared `sb_buf` makes all StringBuilders equivalent.
    /// Reuses a None slot (freed by GC) before growing the backing Vec.
    pub fn alloc(&mut self, class_name: &'static str) -> Option<u16> {
        self.alloc_with_field_count(class_name, default_field_count_for_native(class_name))
    }

    /// Like [`alloc`], but reserves storage for `n_fields` fields up front so
    /// callers that know the field count (native handlers, `op_new` via
    /// [`alloc_with_defaults`]) skip the lazy-grow path inside [`set_field`].
    /// Behaviour is otherwise identical to [`alloc`].
    pub fn alloc_with_field_count(
        &mut self,
        class_name: &'static str,
        n_fields: usize,
    ) -> Option<u16> {
        let class_idx = self.intern_class(class_name);
        if class_name == "java/lang/StringBuilder" {
            if let Some(idx) = self
                .objects
                .iter()
                .position(|o| matches!(o, Some(obj) if obj.class_idx == class_idx))
            {
                return Some(idx as u16);
            }
        }
        Some(self.place_in_slot(JvmObject::new_with_field_count(class_idx, n_fields)))
    }

    /// Look up `name` in [`class_table`] (linear scan), inserting it if
    /// missing. Returns the resulting index. Class-name pointers are
    /// canonical (see [`crate::interpreter::helpers::class_name_to_static_in`])
    /// so byte-equality on `&'static str` is fast.
    fn intern_class(&mut self, name: &'static str) -> u16 {
        for (i, &existing) in self.class_table.iter().enumerate() {
            if existing == name {
                return i as u16;
            }
        }
        let idx = self.class_table.len() as u16;
        self.class_table.push(name);
        idx
    }

    /// Find a free slot (reusing GC-freed None entries before growing) and
    /// place `obj` in it. Returns the slot index.
    fn place_in_slot(&mut self, obj: JvmObject) -> u16 {
        while self.first_free < self.objects.len() {
            if self.objects[self.first_free].is_none() {
                let idx = self.first_free;
                self.objects[idx] = Some(obj);
                self.first_free = idx + 1;
                return idx as u16;
            }
            self.first_free += 1;
        }
        let idx = self.objects.len() as u16;
        self.objects.push(Some(obj));
        self.first_free = self.objects.len();
        idx
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
            let ci = classes
                .iter()
                .position(|cf| cf.class_name().is_some_and(|n| n == current.as_bytes()));
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
                    if current == "java/lang/Enum" {
                        enum_base = true;
                    }
                    break;
                }
            }
        }
        chain.reverse();

        // Size the backing slice exactly once, before allocating, so the
        // `set_field` loop below never triggers a reallocation.
        let n_fields = (if enum_base { ENUM_IMPLICIT_FIELDS } else { 0 })
            + chain
                .iter()
                .map(|&ci| classes[ci].fields().len())
                .sum::<usize>();
        let idx = self.alloc_with_field_count(canonical_name, n_fields)?;

        let mut slot = if enum_base { ENUM_IMPLICIT_FIELDS } else { 0 };
        for ci in chain.iter() {
            let cf = &classes[*ci];
            for fi in cf.fields() {
                let v = match cf.field_descriptor(fi) {
                    Some(desc) => default_for_descriptor(desc),
                    None => Value::Null,
                };
                self.set_field(idx, slot, v)?;
                slot += 1;
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
        let copy = self.objects.get(idx as usize)?.as_ref()?.clone();
        Some(self.place_in_slot(copy))
    }

    pub fn get_field(&self, idx: u16, field: usize) -> Option<Value> {
        self.objects.get(idx as usize)?.as_ref()?.get_field(field)
    }

    pub fn set_field(&mut self, idx: u16, field: usize, v: Value) -> Option<()> {
        self.objects
            .get_mut(idx as usize)?
            .as_mut()?
            .set_field(field, v);
        Some(())
    }

    #[allow(dead_code)]
    pub fn class_name(&self, idx: u16) -> Option<&'static str> {
        let class_idx = self.objects.get(idx as usize)?.as_ref()?.class_idx;
        self.class_table.get(class_idx as usize).copied()
    }

    /// Push a new StringBuilder buffer onto the stack.
    pub fn sb_push(&mut self) {
        self.sb_stack.push(Vec::new());
    }

    /// Pop the top StringBuilder buffer off the stack.
    pub fn sb_pop(&mut self) -> Vec<u8> {
        self.sb_stack.pop().unwrap_or_default()
    }

    /// Append bytes to the top StringBuilder buffer.
    pub fn sb_append_bytes(&mut self, bytes: &[u8]) {
        if let Some(top) = self.sb_stack.last_mut() {
            top.extend_from_slice(bytes);
        }
    }

    /// Append an integer (decimal) to the shared StringBuilder buffer.
    pub fn sb_append_int(&mut self, n: i32) {
        let mut tmp = [0u8; 12];
        let s = int_to_decimal_buf(n, &mut tmp);
        self.sb_append_bytes(s);
    }

    /// Append a long (decimal) to the shared StringBuilder buffer.
    pub fn sb_append_long(&mut self, n: i64) {
        let mut tmp = [0u8; 21];
        let s = long_to_decimal_buf(n, &mut tmp);
        self.sb_append_bytes(s);
    }

    /// Append a float to the shared StringBuilder buffer.
    /// Formats as `[-]integer.fraction` with up to 6 significant decimal digits.
    pub fn sb_append_float(&mut self, f: f32) {
        let mut tmp = [0u8; 32];
        let s = float_to_str_buf(f, &mut tmp);
        self.sb_append_bytes(s);
    }

    /// Return the current length of the top StringBuilder buffer.
    pub fn sb_len(&self) -> usize {
        self.sb_stack.last().map_or(0, |b| b.len())
    }

    /// Return the byte at `idx` in the top StringBuilder buffer, or `None` if out of bounds.
    pub fn sb_char_at(&self, idx: usize) -> Option<u8> {
        self.sb_stack.last()?.get(idx).copied()
    }

    /// Return the top StringBuilder buffer contents as a byte slice.
    pub fn sb_contents_slice(&self) -> &[u8] {
        self.sb_stack.last().map_or(&[], |b| b.as_slice())
    }

    // ── GC support ───────────────────────────────────────────────────────────

    /// Total number of slots (including freed `None` slots).
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
            *slot = None;
            if i < self.first_free {
                self.first_free = i;
            }
        }
    }

    /// Return the slice of populated fields for the object at `idx`, used by
    /// the GC tracer.  Empty when the slot is freed or out of bounds.
    pub fn fields_slice(&self, idx: u16) -> &[Value] {
        self.objects
            .get(idx as usize)
            .and_then(|o| o.as_ref())
            .map(|o| o.fields_slice())
            .unwrap_or(&[])
    }

    /// Approximate bytes held live by this heap. Used by `perfbench` /
    /// `Runtime.usedMemory()` to track heap pressure across optimisation
    /// changes. Sums the per-object slot size plus
    /// `fields.len() * size_of::<Value>()` for the allocated field slice
    /// (capacity, not high-water `field_count`, so we report what's actually
    /// pinned in the allocator).
    ///
    /// Reports DEVICE sizes on every platform: on a 64-bit host
    /// `Option<JvmObject>` inflates to 24 B via the `Box<[Value]>` fat
    /// pointer, and a host-sized figure would misreport what the same app
    /// holds on a real 32-bit RP2040/RP2350 (docs/parity-audit.md
    /// OBJ-01/M5). The cfg'd assert makes every device build re-verify the
    /// constant.
    pub fn live_bytes(&self) -> usize {
        const PER_OBJECT: usize = 12;
        #[cfg(target_pointer_width = "32")]
        const _: () = assert!(core::mem::size_of::<Option<JvmObject>>() == PER_OBJECT);
        const PER_FIELD: usize = core::mem::size_of::<Value>();
        const _: () = assert!(PER_FIELD == 16); // identical on all targets (V1)
        let mut total = 0;
        for i in 0..self.objects.len() {
            if let Some(Some(obj)) = self.objects.get(i) {
                total += PER_OBJECT + obj.fields.len() * PER_FIELD;
            }
        }
        total
    }

    /// Return the top StringBuilder buffer contents as a raw pointer and length.
    ///
    /// # Safety
    /// The returned pointer is valid until the next call to `sb_append_bytes`,
    /// `sb_append_int`, or `sb_push`, any of which may reallocate the buffer.
    pub fn sb_contents(&self) -> (*const u8, usize) {
        match self.sb_stack.last() {
            Some(buf) => (buf.as_ptr(), buf.len()),
            None => (core::ptr::null(), 0),
        }
    }
}

/// Format `n` as a decimal ASCII string into `buf`.  Returns the filled slice.
pub fn long_to_decimal_buf(mut n: i64, buf: &mut [u8; 21]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let neg = n < 0;
    if neg {
        n = n.wrapping_neg();
    }
    let mut i = 21usize;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    &buf[i..]
}

/// Format `n` as a decimal ASCII string into `buf`.  Returns the filled slice.
pub fn int_to_decimal_buf(mut n: i32, buf: &mut [u8; 12]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let neg = n < 0;
    if neg {
        n = n.wrapping_neg();
    }
    let mut i = 12usize;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    &buf[i..]
}

/// Format `f` as a decimal ASCII string into `buf`.
/// Produces `[-]integer.fraction` with up to 6 significant decimal digits.
/// Special values: "NaN", "Infinity", "-Infinity".
pub fn float_to_str_buf(f: f32, buf: &mut [u8; 32]) -> &[u8] {
    if f.is_nan() {
        let s = b"NaN";
        buf[..s.len()].copy_from_slice(s);
        return &buf[..s.len()];
    }
    if f.is_infinite() {
        if f > 0.0 {
            let s = b"Infinity";
            buf[..s.len()].copy_from_slice(s);
            return &buf[..s.len()];
        } else {
            let s = b"-Infinity";
            buf[..s.len()].copy_from_slice(s);
            return &buf[..s.len()];
        }
    }
    let neg = f < 0.0;
    let f = if neg { -f } else { f };

    // Integer and fractional parts
    let int_part = f as u32;
    let frac = f - int_part as f32;

    let mut pos = 0usize;
    if neg {
        buf[pos] = b'-';
        pos += 1;
    }

    // Write integer part
    let mut ibuf = [0u8; 12];
    let istr = int_to_decimal_buf(int_part as i32, &mut ibuf);
    buf[pos..pos + istr.len()].copy_from_slice(istr);
    pos += istr.len();

    // Write up to 6 fractional digits, trimming trailing zeros
    buf[pos] = b'.';
    pos += 1;

    let mut frac_val = (frac * 1_000_000.0 + 0.5) as u32; // round to 6 places
                                                          // Trim trailing zeros
    let mut digits = 6usize;
    while digits > 1 && frac_val % 10 == 0 {
        frac_val /= 10;
        digits -= 1;
    }
    // Write digits (right to left, then reverse)
    let frac_start = pos;
    let mut fv = frac_val;
    for _ in 0..digits {
        buf[pos] = b'0' + (fv % 10) as u8;
        fv /= 10;
        pos += 1;
    }
    buf[frac_start..pos].reverse();

    &buf[..pos]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Value;

    #[test]
    fn alloc_returns_sequential_indices() {
        let mut heap = ObjectHeap::new();
        assert_eq!(heap.alloc("A"), Some(0));
        assert_eq!(heap.alloc("B"), Some(1));
        assert_eq!(heap.alloc("C"), Some(2));
    }

    #[test]
    fn alloc_beyond_old_capacity_succeeds() {
        let mut heap = ObjectHeap::new();
        for i in 0..64 {
            assert!(heap.alloc(if i % 2 == 0 { "X" } else { "Y" }).is_some());
        }
    }

    #[test]
    fn get_field_nonexistent_field_returns_none() {
        let mut heap = ObjectHeap::new();
        heap.alloc("A");
        assert_eq!(heap.get_field(0, 0), None);
    }

    #[test]
    fn set_and_get_field_round_trip() {
        let mut heap = ObjectHeap::new();
        heap.alloc("A");
        heap.set_field(0, 0, Value::Int(42));
        assert_eq!(heap.get_field(0, 0), Some(Value::Int(42)));
    }

    #[test]
    fn set_field_fills_gaps_with_null() {
        let mut heap = ObjectHeap::new();
        heap.alloc("A");
        heap.set_field(0, 2, Value::Int(5));
        assert_eq!(heap.get_field(0, 0), Some(Value::Null));
        assert_eq!(heap.get_field(0, 1), Some(Value::Null));
        assert_eq!(heap.get_field(0, 2), Some(Value::Int(5)));
    }

    #[test]
    fn clone_object_shallow_copies_fields() {
        let mut heap = ObjectHeap::new();
        let src = heap.alloc("Point").unwrap();
        heap.set_field(src, 0, Value::Int(3));
        heap.set_field(src, 1, Value::ArrayRef(7)); // reference field

        let copy = heap.clone_object(src).unwrap();
        assert_ne!(src, copy, "clone must be a distinct object");
        assert_eq!(heap.class_name(copy), Some("Point"));
        assert_eq!(heap.get_field(copy, 0), Some(Value::Int(3)));
        // Shallow: the reference field shares its referent.
        assert_eq!(heap.get_field(copy, 1), Some(Value::ArrayRef(7)));

        // Mutating the copy leaves the original untouched.
        heap.set_field(copy, 0, Value::Int(99));
        assert_eq!(heap.get_field(src, 0), Some(Value::Int(3)));
    }

    #[test]
    fn clone_object_invalid_index_returns_none() {
        let mut heap = ObjectHeap::new();
        assert_eq!(heap.clone_object(42), None);
    }

    #[test]
    fn class_name_returns_correct_name() {
        let mut heap = ObjectHeap::new();
        heap.alloc("MyClass");
        assert_eq!(heap.class_name(0), Some("MyClass"));
    }

    #[test]
    fn class_name_invalid_index_returns_none() {
        let heap = ObjectHeap::new();
        assert_eq!(heap.class_name(99), None);
    }

    #[test]
    fn exception_message_register_get_free() {
        let mut heap = ObjectHeap::new();
        let obj = heap.alloc("java/lang/RuntimeException").unwrap();
        assert_eq!(heap.get_exception_message(obj), None);
        heap.register_exception_message(obj, 7);
        assert_eq!(heap.get_exception_message(obj), Some(7));
        // Re-registering replaces the existing entry.
        heap.register_exception_message(obj, 11);
        assert_eq!(heap.get_exception_message(obj), Some(11));
        heap.free_exception_message(obj);
        assert_eq!(heap.get_exception_message(obj), None);
    }

    #[test]
    fn get_field_invalid_object_returns_none() {
        let heap = ObjectHeap::new();
        assert_eq!(heap.get_field(99, 0), None);
    }

    #[test]
    fn string_builder_reuses_slot() {
        let mut heap = ObjectHeap::new();
        let idx1 = heap.alloc("java/lang/StringBuilder");
        let idx2 = heap.alloc("java/lang/StringBuilder");
        assert!(idx1.is_some());
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn sb_append_bytes_and_contents() {
        let mut heap = ObjectHeap::new();
        heap.sb_push();
        heap.sb_append_bytes(b"hello");
        assert_eq!(heap.sb_contents_slice(), b"hello");
        assert_eq!(heap.sb_len(), 5);
    }

    #[test]
    fn sb_push_creates_fresh_buffer() {
        let mut heap = ObjectHeap::new();
        heap.sb_push();
        heap.sb_append_bytes(b"hello");
        heap.sb_push();
        assert_eq!(heap.sb_len(), 0);
    }

    #[test]
    fn sb_char_at() {
        let mut heap = ObjectHeap::new();
        heap.sb_push();
        heap.sb_append_bytes(b"abc");
        assert_eq!(heap.sb_char_at(0), Some(b'a'));
        assert_eq!(heap.sb_char_at(2), Some(b'c'));
        assert_eq!(heap.sb_char_at(3), None);
    }

    #[test]
    fn sb_append_int_zero() {
        let mut heap = ObjectHeap::new();
        heap.sb_push();
        heap.sb_append_int(0);
        assert_eq!(heap.sb_contents_slice(), b"0");
    }

    #[test]
    fn sb_append_int_positive() {
        let mut heap = ObjectHeap::new();
        heap.sb_push();
        heap.sb_append_int(12345);
        assert_eq!(heap.sb_contents_slice(), b"12345");
    }

    #[test]
    fn sb_append_int_negative() {
        let mut heap = ObjectHeap::new();
        heap.sb_push();
        heap.sb_append_int(-42);
        assert_eq!(heap.sb_contents_slice(), b"-42");
    }

    #[test]
    fn sb_append_no_truncation() {
        let mut heap = ObjectHeap::new();
        heap.sb_push();
        let long_str = [b'x'; 70];
        heap.sb_append_bytes(&long_str);
        assert_eq!(heap.sb_len(), 70);
    }

    #[test]
    fn sb_push_pop_restores_outer() {
        let mut heap = ObjectHeap::new();
        heap.sb_push();
        heap.sb_append_bytes(b"foo");
        heap.sb_append_int(7);
        // Nested builder
        heap.sb_push();
        heap.sb_append_bytes(b"bar");
        let inner = heap.sb_pop();
        assert_eq!(&inner, b"bar");
        // Outer buffer survives
        assert_eq!(heap.sb_contents_slice(), b"foo7");
    }

    #[test]
    fn sb_nested_builders_preserve_content() {
        let mut heap = ObjectHeap::new();
        // Outer: "hi " + (inner) + 42
        heap.sb_push();
        heap.sb_append_bytes(b"hi ");
        // Inner: "Hello, World!" + " bye "
        heap.sb_push();
        heap.sb_append_bytes(b"Hello, World!");
        heap.sb_append_bytes(b" bye ");
        let inner = heap.sb_pop();
        assert_eq!(&inner, b"Hello, World! bye ");
        // Outer resumes
        assert_eq!(heap.sb_contents_slice(), b"hi ");
        heap.sb_append_bytes(&inner);
        heap.sb_append_int(42);
        let outer = heap.sb_pop();
        assert_eq!(&outer, b"hi Hello, World! bye 42");
    }

    #[test]
    fn gc_slot_reuse() {
        let mut heap = ObjectHeap::new();
        assert_eq!(heap.alloc("A"), Some(0));
        assert_eq!(heap.alloc("B"), Some(1));
        // Simulate GC freeing slot 0
        heap.objects[0] = None;
        heap.first_free = 0;
        // Next alloc should reuse slot 0
        assert_eq!(heap.alloc("C"), Some(0));
        // Slot 1 still intact
        assert_eq!(heap.class_name(1), Some("B"));
    }

    #[test]
    fn float_to_str_special() {
        let mut buf = [0u8; 32];
        assert_eq!(float_to_str_buf(f32::NAN, &mut buf), b"NaN");
        assert_eq!(float_to_str_buf(f32::INFINITY, &mut buf), b"Infinity");
        assert_eq!(float_to_str_buf(f32::NEG_INFINITY, &mut buf), b"-Infinity");
    }

    #[test]
    fn float_to_str_zero() {
        let mut buf = [0u8; 32];
        let s = float_to_str_buf(0.0, &mut buf);
        assert_eq!(s, b"0.0");
    }

    #[test]
    fn float_to_str_integer() {
        let mut buf = [0u8; 32];
        let s = float_to_str_buf(42.0, &mut buf);
        // 42.0 → "42.0"
        assert_eq!(s, b"42.0");
    }

    #[test]
    fn float_to_str_negative() {
        let mut buf = [0u8; 32];
        let s = float_to_str_buf(-3.14, &mut buf);
        // Should start with "-3."
        assert!(s.starts_with(b"-3."));
    }

    // ── list_bufs tests ──────────────────────────────────────────────────────

    #[test]
    fn list_alloc_returns_sequential_indices() {
        let mut heap = ObjectHeap::new();
        assert_eq!(heap.list_alloc(), Some(0));
        assert_eq!(heap.list_alloc(), Some(1));
        assert_eq!(heap.list_alloc(), Some(2));
    }

    #[test]
    fn list_add_and_get() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(10));
        heap.list_add(idx, Value::Int(20));
        assert_eq!(heap.list_len(idx), 2);
        assert_eq!(heap.list_get(idx, 0), Some(Value::Int(10)));
        assert_eq!(heap.list_get(idx, 1), Some(Value::Int(20)));
        assert_eq!(heap.list_get(idx, 2), None);
    }

    #[test]
    fn list_set_returns_old_value() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(1));
        let old = heap.list_set(idx, 0, Value::Int(99));
        assert_eq!(old, Some(Value::Int(1)));
        assert_eq!(heap.list_get(idx, 0), Some(Value::Int(99)));
    }

    #[test]
    fn list_remove_returns_value_and_shifts() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(1));
        heap.list_add(idx, Value::Int(2));
        heap.list_add(idx, Value::Int(3));
        let removed = heap.list_remove(idx, 1);
        assert_eq!(removed, Some(Value::Int(2)));
        assert_eq!(heap.list_len(idx), 2);
        assert_eq!(heap.list_get(idx, 1), Some(Value::Int(3)));
    }

    #[test]
    fn list_insert_shifts_right() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(1));
        heap.list_add(idx, Value::Int(3));
        heap.list_insert(idx, 1, Value::Int(2));
        assert_eq!(heap.list_len(idx), 3);
        assert_eq!(heap.list_get(idx, 1), Some(Value::Int(2)));
        assert_eq!(heap.list_get(idx, 2), Some(Value::Int(3)));
    }

    #[test]
    fn list_clear_empties_list() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(1));
        heap.list_add(idx, Value::Int(2));
        heap.list_clear(idx);
        assert_eq!(heap.list_len(idx), 0);
    }

    #[test]
    fn list_free_slot_is_reused() {
        let mut heap = ObjectHeap::new();
        let idx0 = heap.list_alloc().unwrap();
        let _idx1 = heap.list_alloc().unwrap();
        heap.list_free(idx0);
        let reused = heap.list_alloc().unwrap();
        assert_eq!(reused, idx0);
    }

    #[test]
    fn list_iter_yields_elements() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(7));
        heap.list_add(idx, Value::Int(8));
        let collected: alloc::vec::Vec<Value> = heap.list_iter(idx).collect();
        assert_eq!(collected, [Value::Int(7), Value::Int(8)]);
    }
}
