// SPDX-License-Identifier: GPL-3.0-only
use crate::chunked_slots::ChunkedSlots;
use crate::tunables::INLINE_DATA;
use crate::types::Value;
use alloc::vec::Vec;

// JVM atype constants for newarray
pub const ATYPE_BOOLEAN: u8 = 4;
pub const ATYPE_CHAR: u8 = 5;
pub const ATYPE_FLOAT: u8 = 6;
pub const ATYPE_DOUBLE: u8 = 7;
pub const ATYPE_BYTE: u8 = 8;
pub const ATYPE_SHORT: u8 = 9;
pub const ATYPE_INT: u8 = 10;
pub const ATYPE_LONG: u8 = 11;
pub const ATYPE_REF: u8 = 0; // used by anewarray

/// Tag bits for values stored in ATYPE_REF arrays. Arrays store raw i32 but
/// `Value` distinguishes three live reference kinds, so we disambiguate via
/// the top bits. Encoding:
///   - 0                              → Null
///   - OBJ_TAG set                     → ObjectRef (low 16 bits)
///   - REF_TAG set                     → Reference / interned string (low 16 bits)
///   - ARRAY_TAG set                   → ArrayRef (low 16 bits)
///
/// Objects carry a tag too: an untagged `ObjectRef(0)` would be raw 0 —
/// indistinguishable from `Null` — and object slot 0 is an ordinary slot
/// (handed out first, reused after GC), so the first object an executor
/// allocates used to read back as `null` from any `Object[]`.
pub const REF_TAG: u32 = 0x4000_0000;
pub const ARRAY_TAG: u32 = 0x2000_0000;
pub const OBJ_TAG: u32 = 0x1000_0000;

/// Encode a reference `Value` for an `ATYPE_REF` slot per the scheme above;
/// `None` for a non-reference value.
pub fn encode_ref(v: Value) -> Option<i32> {
    Some(match v {
        Value::Null => 0,
        Value::ObjectRef(i) => ((i as u32) | OBJ_TAG) as i32,
        Value::Reference(i) => ((i as u32) | REF_TAG) as i32,
        Value::ArrayRef(i) => ((i as u32) | ARRAY_TAG) as i32,
        _ => return None,
    })
}

/// Decode an `ATYPE_REF` slot back into its reference `Value`.
pub fn decode_ref(raw: i32) -> Value {
    let u = raw as u32;
    if raw == 0 {
        Value::Null
    } else if u & REF_TAG != 0 {
        Value::Reference((u & !REF_TAG) as u16)
    } else if u & ARRAY_TAG != 0 {
        Value::ArrayRef((u & !ARRAY_TAG) as u16)
    } else {
        // OBJ_TAG, or a legacy untagged (non-zero) object index.
        Value::ObjectRef((u & 0xFFFF) as u16)
    }
}

/// Physical i32 slots per user-visible element.
/// `long[]` and `double[]` use two slots per element; everything else uses one.
#[inline]
fn slots_per_elem(atype: u8) -> u16 {
    match atype {
        ATYPE_LONG | ATYPE_DOUBLE => 2,
        _ => 1,
    }
}

/// Element types stored packed at 1 byte per element (in `arena8` /
/// `Inline8`) instead of one i32 slot each — a 75 % payload saving on every
/// `byte[]`/`boolean[]`. Semantics are unchanged: `bastore` already
/// truncates to `i8` before store, and `load` sign-extends back.
/// `char[]`/`short[]` (2 B/elem) are a noted follow-up.
#[inline]
fn is_packed(atype: u8) -> bool {
    matches!(atype, ATYPE_BYTE | ATYPE_BOOLEAN)
}

/// Inline capacity of a packed array, in bytes. Sized to exactly fill the
/// space the i32 `Inline` buffer occupies, so the packed variants cannot
/// grow `JvmArray` (the 40-byte OBJ-05 slot assert).
const INLINE8: usize = INLINE_DATA * 4;

/// Which arena [`ArrayHeap::alloc`] extended, and the length to cut it back
/// to if the slot it was extended for cannot be placed.
enum Rollback {
    Arena(usize),
    Arena8(usize),
}

/// Array data stored either inline (small arrays) or in a shared arena.
///
/// Small arrays are stored inline to avoid arena overhead. Large arrays
/// store an (offset, len) pair pointing into a single contiguous arena Vec,
/// which eliminates per-array FreeRTOS malloc/free churn — the dominant
/// source of heap fragmentation. `byte[]`/`boolean[]` use the packed
/// `Inline8`/`Arena8` forms ([`is_packed`]): 1 byte per element in
/// `ArrayHeap::arena8`; everything else uses i32 slots in
/// `ArrayHeap::arena`.
enum ArrayData {
    Inline {
        buf: [i32; INLINE_DATA],
        len: u16,
    },
    Arena {
        offset: u32,
        len: u16,
    },
    /// Packed small `byte[]`/`boolean[]`; `len` is in bytes (= elements).
    Inline8 {
        buf: [u8; INLINE8],
        len: u16,
    },
    /// Packed large `byte[]`/`boolean[]` span in `ArrayHeap::arena8`.
    Arena8 {
        offset: u32,
        len: u16,
    },
}

struct JvmArray {
    pub atype: u8,
    data: ArrayData,
}

pub struct ArrayHeap {
    arrays: ChunkedSlots<JvmArray>,
    /// Lowest index that might contain a `None` slot; avoids O(n) scans.
    first_free: usize,
    /// Contiguous arena for large-array element data.
    /// All `ArrayData::Arena` entries index into this Vec.
    arena: Vec<i32>,
    /// Contiguous byte arena for packed `byte[]`/`boolean[]` payloads.
    /// All `ArrayData::Arena8` entries index into this Vec. Compacted after
    /// GC like `arena`.
    arena8: Vec<u8>,
    /// Allocations since the interpreter last folded this into GC pacing
    /// (see `Executor::fold_native_alloc_events`).
    alloc_events: u16,
}

impl ArrayHeap {
    pub const fn new() -> Self {
        Self {
            arrays: ChunkedSlots::new(),
            first_free: 0,
            arena: Vec::new(),
            arena8: Vec::new(),
            alloc_events: 0,
        }
    }

    /// Drain the pacing counter (see `alloc_events`).
    pub fn take_alloc_events(&mut self) -> u16 {
        core::mem::take(&mut self.alloc_events)
    }
}

impl Default for ArrayHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrayHeap {
    /// Allocate a new array. `len` is the user-visible element count.
    /// Returns its heap index.
    /// Reuses a None slot (freed by GC) before growing the backing Vec.
    /// Small arrays (<= 8 slots) use inline storage; larger arrays
    /// append their data to the shared arena.
    ///
    /// `long[]` and `double[]` occupy two i32 slots per element, so the
    /// physical slot count is `2 * len` for ATYPE_LONG / ATYPE_DOUBLE.
    pub fn alloc(&mut self, atype: u8, len: u16) -> Option<u16> {
        // Arena reservation + slot placement must be scheduler-atomic —
        // same interleave hazard as ObjectHeap::alloc_span (see
        // `atomic_section` module docs).
        let _atomic = crate::atomic_section::AtomicSection::enter();
        self.alloc_events = self.alloc_events.saturating_add(1);
        // Where to cut an arena back to if the slot push below fails.
        let mut rollback: Option<Rollback> = None;
        let data = if is_packed(atype) {
            // Packed byte/boolean payload: 1 byte per element.
            if (len as usize) <= INLINE8 {
                ArrayData::Inline8 {
                    buf: [0u8; INLINE8],
                    len,
                }
            } else {
                // try_reserve_exact — see the i32-arena comment below.
                if self.arena8.try_reserve_exact(len as usize).is_err() {
                    return None; // OOM — caller should trigger GC and retry
                }
                let offset = self.arena8.len() as u32;
                rollback = Some(Rollback::Arena8(self.arena8.len()));
                self.arena8.resize(self.arena8.len() + len as usize, 0u8);
                ArrayData::Arena8 { offset, len }
            }
        } else {
            let slots_per_elem = slots_per_elem(atype) as u32;
            let phys_u32 = (len as u32).checked_mul(slots_per_elem)?;
            if phys_u32 > u16::MAX as u32 {
                return None;
            }
            let phys = phys_u32 as u16;
            if (phys as usize) <= INLINE_DATA {
                ArrayData::Inline {
                    buf: [0i32; INLINE_DATA],
                    len: phys,
                }
            } else {
                let extra = phys as usize;
                // Use try_reserve_exact to avoid Vec's amortized 2× growth.
                // On constrained FreeRTOS heaps the doubling can request more
                // contiguous memory than is available (e.g. 64 KB → 128 KB).
                if self.arena.try_reserve_exact(extra).is_err() {
                    return None; // OOM — caller should trigger GC and retry
                }
                let offset = self.arena.len() as u32;
                rollback = Some(Rollback::Arena(self.arena.len()));
                self.arena.resize(self.arena.len() + extra, 0i32);
                ArrayData::Arena { offset, len: phys }
            }
        };
        let new_arr = JvmArray { atype, data };
        // Scan from first_free for a None slot; skip already-occupied prefix.
        while self.first_free < self.arrays.len() {
            if self.arrays[self.first_free].is_none() {
                let idx = self.first_free;
                self.arrays[idx] = Some(new_arr);
                self.first_free = idx + 1;
                return Some(idx as u16);
            }
            self.first_free += 1;
        }
        let idx = self.arrays.len() as u16;
        // `try_push`, not `push`: a slot past the tail chunk allocates a
        // whole new chunk, and `push` does that infallibly — which aborts
        // the board under a full heap, exactly where the two arena
        // reservations above are careful to answer `None` instead. Every
        // caller already treats `None` as "GC and retry", so the last step
        // of an allocation must not be the one that panics.
        if self.arrays.try_push(Some(new_arr)).is_none() {
            // The payload was reserved before the slot; hand it back rather
            // than stranding it in the arena until the next compaction.
            match rollback {
                Some(Rollback::Arena(len)) => self.arena.truncate(len),
                Some(Rollback::Arena8(len)) => self.arena8.truncate(len),
                None => {}
            }
            return None;
        }
        self.first_free = self.arrays.len();
        Some(idx)
    }

    /// Load element at index `elem` from array `idx`. Packed byte/boolean
    /// elements are sign-extended (`b as i8 as i32`) — for booleans (0/1)
    /// this is the identity, for bytes it matches `baload`.
    pub fn load(&self, idx: u16, elem: usize) -> Option<i32> {
        let arr = self.arrays.get(idx as usize)?.as_ref()?;
        match &arr.data {
            ArrayData::Inline { buf, len } => {
                if elem >= *len as usize {
                    return None;
                }
                Some(buf[elem])
            }
            ArrayData::Arena { offset, len } => {
                if elem >= *len as usize {
                    return None;
                }
                Some(self.arena[*offset as usize + elem])
            }
            ArrayData::Inline8 { buf, len } => {
                if elem >= *len as usize {
                    return None;
                }
                Some(buf[elem] as i8 as i32)
            }
            ArrayData::Arena8 { offset, len } => {
                if elem >= *len as usize {
                    return None;
                }
                Some(self.arena8[*offset as usize + elem] as i8 as i32)
            }
        }
    }

    /// Store value at index `elem` in array `idx`. Packed byte/boolean
    /// storage keeps the low 8 bits — `bastore` truncates to `i8` before
    /// calling here, so nothing observable changes.
    pub fn store(&mut self, idx: u16, elem: usize, val: i32) -> Option<()> {
        // Read the data variant and copy out what we need, releasing the
        // immutable borrow on self.arrays before mutating.
        let arr = self.arrays.get(idx as usize)?.as_ref()?;
        enum Loc {
            Inline,
            Arena(u32),
            Inline8,
            Arena8(u32),
        }
        let (loc, len) = match &arr.data {
            ArrayData::Inline { len, .. } => (Loc::Inline, *len),
            ArrayData::Arena { offset, len } => (Loc::Arena(*offset), *len),
            ArrayData::Inline8 { len, .. } => (Loc::Inline8, *len),
            ArrayData::Arena8 { offset, len } => (Loc::Arena8(*offset), *len),
        };
        if elem >= len as usize {
            return None;
        }
        match loc {
            Loc::Inline => {
                if let Some(Some(arr)) = self.arrays.get_mut(idx as usize) {
                    if let ArrayData::Inline { buf, .. } = &mut arr.data {
                        buf[elem] = val;
                    }
                }
            }
            Loc::Arena(offset) => self.arena[offset as usize + elem] = val,
            Loc::Inline8 => {
                if let Some(Some(arr)) = self.arrays.get_mut(idx as usize) {
                    if let ArrayData::Inline8 { buf, .. } = &mut arr.data {
                        buf[elem] = val as u8;
                    }
                }
            }
            Loc::Arena8(offset) => self.arena8[offset as usize + elem] = val as u8,
        }
        Some(())
    }

    /// Return the user-visible length of array `idx`.
    /// For `long[]` / `double[]` this is the underlying slot count divided by 2.
    pub fn length(&self, idx: u16) -> Option<u16> {
        let arr = self.arrays.get(idx as usize)?.as_ref()?;
        let phys = match &arr.data {
            ArrayData::Inline { len, .. } => *len,
            ArrayData::Arena { len, .. } => *len,
            // Packed: len is bytes = elements; slots_per_elem is 1.
            ArrayData::Inline8 { len, .. } => *len,
            ArrayData::Arena8 { len, .. } => *len,
        };
        Some(phys / slots_per_elem(arr.atype))
    }

    /// Load a 64-bit value from a `long[]` / `double[]` at user-visible
    /// index `elem`. Bytes are stored little-endian across two i32 slots
    /// (lower 32 bits first).
    pub fn load64(&self, idx: u16, elem: usize) -> Option<i64> {
        let raw = elem.checked_mul(2)?;
        let lo = self.load(idx, raw)? as u32 as u64;
        let hi = self.load(idx, raw + 1)? as u32 as u64;
        Some(((hi << 32) | lo) as i64)
    }

    /// Store a 64-bit value into a `long[]` / `double[]` at user-visible
    /// index `elem`. Bytes are stored little-endian across two i32 slots.
    pub fn store64(&mut self, idx: u16, elem: usize, val: i64) -> Option<()> {
        let raw = elem.checked_mul(2)?;
        let bits = val as u64;
        self.store(idx, raw, bits as u32 as i32)?;
        self.store(idx, raw + 1, (bits >> 32) as u32 as i32)?;
        Some(())
    }

    pub fn atype(&self, idx: u16) -> Option<u8> {
        Some(self.arrays.get(idx as usize)?.as_ref()?.atype)
    }

    /// Clone an array: allocate a new array with the same atype/length and
    /// copy all elements. Returns the new array's index, or `None` on OOM.
    pub fn clone(&mut self, idx: u16) -> Option<u16> {
        let atype = self.atype(idx)?;
        let len = self.length(idx)?;
        // Copy the data into a temporary buffer before allocating (to avoid
        // borrowing conflicts during allocation).
        if is_packed(atype) {
            let data: alloc::vec::Vec<u8> = self.packed_slice(idx).to_vec();
            let new_idx = self.alloc(atype, len)?;
            for (i, b) in data.iter().enumerate() {
                self.store(new_idx, i, *b as i8 as i32);
            }
            Some(new_idx)
        } else {
            let data: alloc::vec::Vec<i32> = self.data_slice(idx).to_vec();
            let new_idx = self.alloc(atype, len)?;
            for (i, v) in data.iter().enumerate() {
                self.store(new_idx, i, *v);
            }
            Some(new_idx)
        }
    }

    /// Raw byte view of a packed array's payload (empty for non-packed).
    fn packed_slice(&self, idx: u16) -> &[u8] {
        match self.arrays.get(idx as usize).and_then(|a| a.as_ref()) {
            Some(arr) => match &arr.data {
                ArrayData::Inline8 { buf, len } => &buf[..*len as usize],
                ArrayData::Arena8 { offset, len } => {
                    let o = *offset as usize;
                    &self.arena8[o..o + *len as usize]
                }
                _ => &[],
            },
            None => &[],
        }
    }

    // ── GC support ────────────────────────────────────────────────────────────

    /// Total number of slots (including freed `None` slots).
    /// Allocated slot chunks (diagnostics / pre-reservation sizing).
    pub fn slot_chunk_count(&self) -> usize {
        self.arrays.chunk_count()
    }

    /// Current payload-arena capacity in `i32` slots.
    pub fn arena_capacity(&self) -> usize {
        self.arena.capacity()
    }

    /// Current packed byte-arena capacity in bytes.
    pub fn arena8_capacity(&self) -> usize {
        self.arena8.capacity()
    }

    /// Boot-time pre-reservation of slot chunks + payload-arena capacity —
    /// see `ObjectHeap::prereserve` for the fragmentation rationale.
    /// `arena8_bytes` sizes the packed byte-array arena.
    pub fn prereserve(&mut self, slot_chunks: usize, arena_values: usize, arena8_bytes: usize) {
        self.arrays.reserve_chunks(slot_chunks);
        let target = arena_values.saturating_sub(self.arena.len());
        if self.arena.capacity() < arena_values {
            let _ = self.arena.try_reserve_exact(target);
        }
        let target8 = arena8_bytes.saturating_sub(self.arena8.len());
        if self.arena8.capacity() < arena8_bytes {
            let _ = self.arena8.try_reserve_exact(target8);
        }
    }

    pub fn slot_count(&self) -> usize {
        self.arrays.len()
    }

    /// Returns `true` if the slot at `idx` contains a live array.
    pub fn is_live(&self, idx: u16) -> bool {
        self.arrays.get(idx as usize).is_some_and(|a| a.is_some())
    }

    /// Approximate bytes held live by this heap (slot overhead + arena
    /// payload of live arrays). Inline-array data is folded into the slot
    /// overhead via `size_of::<Option<JvmArray>>()`. Used by `perfbench` /
    /// `Runtime.usedMemory()` to track heap pressure across optimisation
    /// changes.
    pub fn live_bytes(&self) -> usize {
        const PER_SLOT: usize = core::mem::size_of::<Option<JvmArray>>();
        // Pointer-free layout: identical on 32-bit device and 64-bit host
        // (verified across all targets — docs/parity-audit.md OBJ-05). If
        // this assert moves, usedMemory parity needs re-auditing.
        const _: () = assert!(PER_SLOT == 40);
        let mut total = 0;
        for i in 0..self.arrays.len() {
            if let Some(Some(arr)) = self.arrays.get(i) {
                total += PER_SLOT;
                match &arr.data {
                    ArrayData::Arena { len, .. } => {
                        total += (*len as usize) * core::mem::size_of::<i32>();
                    }
                    // Packed payload: 1 byte per element.
                    ArrayData::Arena8 { len, .. } => total += *len as usize,
                    ArrayData::Inline { .. } | ArrayData::Inline8 { .. } => {}
                }
            }
        }
        total
    }

    /// Current payload-arena length in `i32` slots (live spans + dead spans
    /// awaiting compaction; `arena_capacity() - arena_len()` is reserved
    /// slack).
    #[cfg(feature = "mem-diag")]
    pub fn arena_len(&self) -> usize {
        self.arena.len()
    }

    /// Live-set census bucketed by JVM `atype` (index = atype constant;
    /// only 0 and 4..=11 are populated). Splits the [`Self::live_bytes`]
    /// accounting into slot overhead vs arena payload per element type, and
    /// counts how many arrays are inline (arena-free). Fixed-size output —
    /// no allocation (monitor rule).
    #[cfg(feature = "mem-diag")]
    pub fn census_by_atype(&self) -> [AtypeCensus; 12] {
        const PER_SLOT: u32 = core::mem::size_of::<Option<JvmArray>>() as u32;
        let mut out = [AtypeCensus::default(); 12];
        for i in 0..self.arrays.len() {
            if let Some(Some(arr)) = self.arrays.get(i) {
                let Some(row) = out.get_mut(arr.atype as usize) else {
                    continue;
                };
                row.count += 1;
                row.slot_bytes += PER_SLOT;
                match &arr.data {
                    ArrayData::Inline { .. } | ArrayData::Inline8 { .. } => row.inline_count += 1,
                    ArrayData::Arena { len, .. } => {
                        row.arena_bytes += *len as u32 * core::mem::size_of::<i32>() as u32;
                    }
                    ArrayData::Arena8 { len, .. } => row.arena_bytes += *len as u32,
                }
            }
        }
        out
    }

    /// Free the array at `idx`, setting its slot to `None`.
    /// Arena space is NOT reclaimed here — it is reclaimed during compaction.
    pub fn free(&mut self, idx: u16) {
        let i = idx as usize;
        if let Some(slot) = self.arrays.get_mut(i) {
            // Offensive mode: poison the freed payload (arena span or inline
            // buffer) so stale reads surface as the pattern, not as
            // plausible stale data. See `ObjectHeap::free`.
            #[cfg(feature = "mem-diag")]
            if crate::mem_diag::offensive() {
                if let Some(arr) = slot.as_mut() {
                    match &mut arr.data {
                        ArrayData::Inline { buf, .. } => {
                            buf.fill(crate::mem_diag::POISON_I32);
                        }
                        ArrayData::Arena { offset, len } => {
                            let start = *offset as usize;
                            let end = start + *len as usize;
                            if end <= self.arena.len() {
                                self.arena[start..end].fill(crate::mem_diag::POISON_I32);
                            }
                        }
                        ArrayData::Inline8 { buf, .. } => {
                            buf.fill(crate::mem_diag::POISON_BYTE);
                        }
                        ArrayData::Arena8 { offset, len } => {
                            let start = *offset as usize;
                            let end = start + *len as usize;
                            if end <= self.arena8.len() {
                                self.arena8[start..end].fill(crate::mem_diag::POISON_BYTE);
                            }
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

    /// Structural integrity sweep (mem-diag offensive mode): live arena
    /// spans in-bounds and non-overlapping, `first_free` consistent, chunk
    /// store consistent. Mirrors `ObjectHeap::integrity_check`.
    #[cfg(feature = "mem-diag")]
    pub fn integrity_check(&self) -> Result<(), &'static str> {
        if !self.arrays.invariant_holds() {
            return Err("ArrayHeap: ChunkedSlots chunk/len invariant broken");
        }
        for i in 0..self.first_free.min(self.arrays.len()) {
            if self.arrays[i].is_none() {
                return Err("ArrayHeap: free slot below first_free");
            }
        }
        // Two independent span spaces: i32 arena and packed byte arena.
        // Spans are only compared within their own arena.
        let arena_len = self.arena.len();
        let arena8_len = self.arena8.len();
        let span = |arr: &JvmArray| -> Option<(bool, usize, usize)> {
            match &arr.data {
                ArrayData::Arena { offset, len } => Some((false, *offset as usize, *len as usize)),
                ArrayData::Arena8 { offset, len } => Some((true, *offset as usize, *len as usize)),
                ArrayData::Inline { .. } | ArrayData::Inline8 { .. } => None,
            }
        };
        for (i, slot) in self.arrays.iter().enumerate() {
            let Some(a) = slot.as_ref() else { continue };
            let Some((a_packed, a_start, a_len)) = span(a) else {
                continue;
            };
            let bound = if a_packed { arena8_len } else { arena_len };
            if a_start + a_len > bound {
                return Err("ArrayHeap: data span out of arena bounds");
            }
            if a_len == 0 {
                continue;
            }
            for slot_b in self.arrays.iter().skip(i + 1) {
                let Some(b) = slot_b.as_ref() else { continue };
                let Some((b_packed, b_start, b_len)) = span(b) else {
                    continue;
                };
                if b_len == 0 || b_packed != a_packed {
                    continue;
                }
                if a_start < b_start + b_len && b_start < a_start + a_len {
                    return Err("ArrayHeap: overlapping arena spans");
                }
            }
        }
        Ok(())
    }

    /// Return the raw i32-slot data slice of the array at `idx` (for
    /// ATYPE_REF scanning by the GC, and `clone`). Packed byte/boolean
    /// arrays have no i32 slots and return `&[]` — their only callers are
    /// the ref-array tracer (never packed) and the packed `clone` arm.
    pub fn data_slice(&self, idx: u16) -> &[i32] {
        match self.arrays.get(idx as usize).and_then(|a| a.as_ref()) {
            Some(arr) => match &arr.data {
                ArrayData::Inline { buf, len } => &buf[..*len as usize],
                ArrayData::Arena { offset, len } => {
                    let o = *offset as usize;
                    &self.arena[o..o + *len as usize]
                }
                ArrayData::Inline8 { .. } | ArrayData::Arena8 { .. } => &[],
            },
            None => &[],
        }
    }

    /// Compact the arena by sliding live array data down to fill gaps left by
    /// freed arrays. Called by GC after sweep.
    ///
    /// `buf` is a reusable scratch buffer (owned by `GcState`) to avoid
    /// allocating during compaction. Entries are packed into the `u64` sort
    /// key described in [`crate::sort`] — offset in the high half so the sort
    /// orders by it, then the slot index and length — so this shares the
    /// JVM's single sort instantiation instead of monomorphising another one
    /// for a tuple.
    pub fn compact_arena(&mut self, buf: &mut Vec<u64>) {
        buf.clear();
        // Size the scratch buffer fallibly for the larger pass: a heap too
        // full to hold it skips the compaction (the dead spans wait for a
        // later cycle) rather than aborting inside the collector.
        let (mut arena_live, mut arena8_live) = (0usize, 0usize);
        for arr in self.arrays.iter().flatten() {
            match arr.data {
                ArrayData::Arena { .. } => arena_live += 1,
                ArrayData::Arena8 { .. } => arena8_live += 1,
                _ => {}
            }
        }
        if buf.try_reserve(arena_live.max(arena8_live)).is_err() {
            return;
        }
        for (i, slot) in self.arrays.iter().enumerate() {
            if let Some(arr) = slot.as_ref() {
                if let ArrayData::Arena { offset, len } = &arr.data {
                    // Slots are addressed by `ArrayRef(u16)`, so an index
                    // always fits the 16 bits reserved for it here.
                    debug_assert!(i <= u16::MAX as usize, "array slot index overflows the key");
                    buf.push(((*offset as u64) << 32) | ((i as u64) << 16) | *len as u64);
                }
            }
        }
        // Sort by arena offset so we slide data forward in order.
        crate::sort::sort_keys(buf);

        let mut write_pos: usize = 0;
        for &key in buf.iter() {
            let (slot_idx, read_offset, len) = (
                (key >> 16) as usize & 0xffff,
                (key >> 32) as u32,
                key as u16,
            );
            let read_pos = read_offset as usize;
            let count = len as usize;
            if read_pos != write_pos {
                self.arena
                    .copy_within(read_pos..read_pos + count, write_pos);
            }
            if let Some(Some(arr)) = self.arrays.get_mut(slot_idx) {
                if let ArrayData::Arena { offset, .. } = &mut arr.data {
                    *offset = write_pos as u32;
                }
            }
            write_pos += count;
        }
        self.arena.truncate(write_pos);

        // Second pass: the packed byte arena, same scratch buffer.
        buf.clear();
        for (i, slot) in self.arrays.iter().enumerate() {
            if let Some(arr) = slot.as_ref() {
                if let ArrayData::Arena8 { offset, len } = &arr.data {
                    debug_assert!(i <= u16::MAX as usize, "array slot index overflows the key");
                    buf.push(((*offset as u64) << 32) | ((i as u64) << 16) | *len as u64);
                }
            }
        }
        crate::sort::sort_keys(buf);

        let mut write_pos: usize = 0;
        for &key in buf.iter() {
            let (slot_idx, read_offset, len) = (
                (key >> 16) as usize & 0xffff,
                (key >> 32) as u32,
                key as u16,
            );
            let read_pos = read_offset as usize;
            let count = len as usize;
            if read_pos != write_pos {
                self.arena8
                    .copy_within(read_pos..read_pos + count, write_pos);
            }
            if let Some(Some(arr)) = self.arrays.get_mut(slot_idx) {
                if let ArrayData::Arena8 { offset, .. } = &mut arr.data {
                    *offset = write_pos as u32;
                }
            }
            write_pos += count;
        }
        self.arena8.truncate(write_pos);
    }
}

/// One row of the live-array census, per JVM `atype` (see
/// [`ArrayHeap::census_by_atype`]).
#[cfg(feature = "mem-diag")]
#[derive(Clone, Copy, Default)]
pub struct AtypeCensus {
    pub count: u32,
    pub slot_bytes: u32,
    pub arena_bytes: u32,
    pub inline_count: u32,
}

#[cfg(test)]
mod tests;
