use alloc::vec::Vec;

// JVM atype constants for newarray
#[allow(dead_code)]
pub const ATYPE_BOOLEAN: u8 = 4;
#[allow(dead_code)]
pub const ATYPE_CHAR: u8 = 5;
#[allow(dead_code)]
pub const ATYPE_FLOAT: u8 = 6;
#[allow(dead_code)]
pub const ATYPE_DOUBLE: u8 = 7;
#[allow(dead_code)]
pub const ATYPE_BYTE: u8 = 8;
#[allow(dead_code)]
pub const ATYPE_SHORT: u8 = 9;
#[allow(dead_code)]
pub const ATYPE_INT: u8 = 10;
#[allow(dead_code)]
pub const ATYPE_LONG: u8 = 11;
pub const ATYPE_REF: u8 = 0; // used by anewarray

/// Maximum number of elements stored inline (no heap allocation).
const INLINE_DATA: usize = 8;

/// Array data stored either inline (small arrays) or in the shared arena.
///
/// Small arrays (<= 8 elements) are stored inline to avoid arena overhead.
/// Large arrays store an (offset, len) pair pointing into `ArrayHeap::arena`,
/// a single contiguous `Vec<i32>` that eliminates per-array FreeRTOS
/// malloc/free churn — the dominant source of heap fragmentation.
enum ArrayData {
    Inline { buf: [i32; INLINE_DATA], len: u16 },
    Arena { offset: u32, len: u16 },
}

struct JvmArray {
    pub atype: u8,
    data: ArrayData,
}

pub struct ArrayHeap {
    arrays: Vec<Option<JvmArray>>,
    /// Lowest index that might contain a `None` slot; avoids O(n) scans.
    first_free: usize,
    /// Contiguous arena for large-array element data.
    /// All `ArrayData::Arena` entries index into this Vec.
    arena: Vec<i32>,
}

impl ArrayHeap {
    pub const fn new() -> Self {
        Self {
            arrays: Vec::new(),
            first_free: 0,
            arena: Vec::new(),
        }
    }
}

impl Default for ArrayHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrayHeap {
    /// Allocate a new array. Returns its heap index.
    /// Reuses a None slot (freed by GC) before growing the backing Vec.
    /// Small arrays (<= 8 elements) use inline storage; larger arrays
    /// append their data to the shared arena.
    pub fn alloc(&mut self, atype: u8, len: u16) -> Option<u16> {
        let data = if (len as usize) <= INLINE_DATA {
            ArrayData::Inline {
                buf: [0i32; INLINE_DATA],
                len,
            }
        } else {
            let extra = len as usize;
            // Use try_reserve_exact to avoid Vec's amortized 2× growth.
            // On constrained FreeRTOS heaps the doubling can request more
            // contiguous memory than is available (e.g. 64 KB → 128 KB).
            if self.arena.try_reserve_exact(extra).is_err() {
                return None; // OOM — caller should trigger GC and retry
            }
            let offset = self.arena.len() as u32;
            self.arena.resize(self.arena.len() + extra, 0i32);
            ArrayData::Arena { offset, len }
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
        self.arrays.push(Some(new_arr));
        self.first_free = self.arrays.len();
        Some(idx)
    }

    /// Load element at index `elem` from array `idx`.
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
        }
    }

    /// Store value at index `elem` in array `idx`.
    pub fn store(&mut self, idx: u16, elem: usize, val: i32) -> Option<()> {
        // Read the data variant and copy out what we need, releasing the
        // immutable borrow on self.arrays before mutating.
        let arr = self.arrays.get(idx as usize)?.as_ref()?;
        let (is_inline, offset, len) = match &arr.data {
            ArrayData::Inline { len, .. } => (true, 0u32, *len),
            ArrayData::Arena { offset, len } => (false, *offset, *len),
        };
        if elem >= len as usize {
            return None;
        }
        if is_inline {
            if let Some(Some(arr)) = self.arrays.get_mut(idx as usize) {
                if let ArrayData::Inline { buf, .. } = &mut arr.data {
                    buf[elem] = val;
                }
            }
        } else {
            self.arena[offset as usize + elem] = val;
        }
        Some(())
    }

    /// Return the length of array `idx`.
    pub fn length(&self, idx: u16) -> Option<u16> {
        let arr = self.arrays.get(idx as usize)?.as_ref()?;
        Some(match &arr.data {
            ArrayData::Inline { len, .. } => *len,
            ArrayData::Arena { len, .. } => *len,
        })
    }

    #[allow(dead_code)]
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
        let data: alloc::vec::Vec<i32> = self.data_slice(idx).to_vec();
        let new_idx = self.alloc(atype, len)?;
        for (i, v) in data.iter().enumerate() {
            self.store(new_idx, i, *v);
        }
        Some(new_idx)
    }

    // ── GC support ────────────────────────────────────────────────────────────

    /// Total number of slots (including freed `None` slots).
    pub fn slot_count(&self) -> usize {
        self.arrays.len()
    }

    /// Returns `true` if the slot at `idx` contains a live array.
    pub fn is_live(&self, idx: u16) -> bool {
        self.arrays.get(idx as usize).is_some_and(|a| a.is_some())
    }

    /// Free the array at `idx`, setting its slot to `None`.
    /// Arena space is NOT reclaimed here — it is reclaimed during compaction.
    pub fn free(&mut self, idx: u16) {
        let i = idx as usize;
        if let Some(slot) = self.arrays.get_mut(i) {
            *slot = None;
            if i < self.first_free {
                self.first_free = i;
            }
        }
    }

    /// Return the raw data slice of the array at `idx` (for ATYPE_REF scanning).
    pub fn data_slice(&self, idx: u16) -> &[i32] {
        match self.arrays.get(idx as usize).and_then(|a| a.as_ref()) {
            Some(arr) => match &arr.data {
                ArrayData::Inline { buf, len } => &buf[..*len as usize],
                ArrayData::Arena { offset, len } => {
                    let o = *offset as usize;
                    &self.arena[o..o + *len as usize]
                }
            },
            None => &[],
        }
    }

    /// Compact the arena by sliding live array data down to fill gaps left by
    /// freed arrays. Called by GC after sweep.
    ///
    /// `buf` is a reusable scratch buffer (owned by `GcState`) to avoid
    /// allocating during compaction.
    pub fn compact_arena(&mut self, buf: &mut Vec<(usize, u32, u16)>) {
        buf.clear();
        for (i, slot) in self.arrays.iter().enumerate() {
            if let Some(arr) = slot.as_ref() {
                if let ArrayData::Arena { offset, len } = &arr.data {
                    buf.push((i, *offset, *len));
                }
            }
        }
        // Sort by arena offset so we slide data forward in order.
        buf.sort_unstable_by_key(|&(_, offset, _)| offset);

        let mut write_pos: usize = 0;
        for &(slot_idx, read_offset, len) in buf.iter() {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_returns_sequential_indices() {
        let mut heap = ArrayHeap::new();
        assert_eq!(heap.alloc(ATYPE_INT, 4), Some(0));
        assert_eq!(heap.alloc(ATYPE_BYTE, 8), Some(1));
        assert_eq!(heap.alloc(ATYPE_CHAR, 2), Some(2));
    }

    #[test]
    fn alloc_beyond_old_capacity_succeeds() {
        let mut heap = ArrayHeap::new();
        for i in 0..64u16 {
            assert_eq!(heap.alloc(ATYPE_INT, 1), Some(i));
        }
    }

    #[test]
    fn alloc_large_array_succeeds() {
        let mut heap = ArrayHeap::new();
        assert_eq!(heap.alloc(ATYPE_INT, 1000), Some(0));
        assert_eq!(heap.length(0), Some(1000));
    }

    #[test]
    fn alloc_zero_length_succeeds() {
        let mut heap = ArrayHeap::new();
        assert_eq!(heap.alloc(ATYPE_INT, 0), Some(0));
        assert_eq!(heap.length(0), Some(0));
    }

    #[test]
    fn length_returns_correct_value() {
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_INT, 7);
        assert_eq!(heap.length(0), Some(7));
    }

    #[test]
    fn store_and_load_int_roundtrip() {
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_INT, 4);
        assert_eq!(heap.store(0, 2, 99), Some(()));
        assert_eq!(heap.load(0, 2), Some(99));
    }

    #[test]
    fn elements_default_to_zero() {
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_INT, 4);
        assert_eq!(heap.load(0, 0), Some(0));
        assert_eq!(heap.load(0, 3), Some(0));
    }

    #[test]
    fn load_out_of_bounds_returns_none() {
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_INT, 3);
        assert_eq!(heap.load(0, 3), None);
        assert_eq!(heap.load(0, 10), None);
    }

    #[test]
    fn store_out_of_bounds_returns_none() {
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_INT, 3);
        assert_eq!(heap.store(0, 3, 1), None);
    }

    #[test]
    fn load_invalid_array_index_returns_none() {
        let heap = ArrayHeap::new();
        assert_eq!(heap.load(99, 0), None);
    }

    #[test]
    fn byte_sign_extension_semantics() {
        // Store -128 as byte (i8), load back as i32 should be -128
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_BYTE, 2);
        // Store raw i32 value that represents byte -128
        heap.store(0, 0, -128i32);
        let raw = heap.load(0, 0).unwrap();
        let as_byte = raw as i8 as i32;
        assert_eq!(as_byte, -128);
    }

    #[test]
    fn char_zero_extension_semantics() {
        // Store 0xFFFF as char, load back as i32 zero-extended should be 65535
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_CHAR, 1);
        heap.store(0, 0, 0xFFFFu16 as i32);
        let raw = heap.load(0, 0).unwrap();
        let as_char = raw as u16 as i32;
        assert_eq!(as_char, 65535);
    }

    #[test]
    fn atype_returns_correct_value() {
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_BYTE, 4);
        heap.alloc(ATYPE_CHAR, 2);
        assert_eq!(heap.atype(0), Some(ATYPE_BYTE));
        assert_eq!(heap.atype(1), Some(ATYPE_CHAR));
    }

    #[test]
    fn gc_slot_reuse() {
        let mut heap = ArrayHeap::new();
        assert_eq!(heap.alloc(ATYPE_INT, 4), Some(0));
        assert_eq!(heap.alloc(ATYPE_INT, 8), Some(1));
        // Simulate GC freeing slot 0
        heap.arrays[0] = None;
        heap.first_free = 0;
        // Next alloc should reuse slot 0
        assert_eq!(heap.alloc(ATYPE_BYTE, 2), Some(0));
        // Slot 1 still intact
        assert_eq!(heap.length(1), Some(8));
    }

    // ── Arena-backed array tests ────────────────────────────────────────────

    #[test]
    fn arena_load_store_roundtrip() {
        let mut heap = ArrayHeap::new();
        // 20 elements > INLINE_DATA(8) → arena-backed
        heap.alloc(ATYPE_INT, 20);
        for i in 0..20 {
            assert_eq!(heap.store(0, i, (i * 10) as i32), Some(()));
        }
        for i in 0..20 {
            assert_eq!(heap.load(0, i), Some((i * 10) as i32));
        }
    }

    #[test]
    fn arena_data_slice() {
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_INT, 10);
        heap.store(0, 0, 100);
        heap.store(0, 9, 999);
        let slice = heap.data_slice(0);
        assert_eq!(slice.len(), 10);
        assert_eq!(slice[0], 100);
        assert_eq!(slice[9], 999);
    }

    #[test]
    fn arena_multiple_arrays() {
        let mut heap = ArrayHeap::new();
        heap.alloc(ATYPE_INT, 10); // slot 0, arena [0..10)
        heap.alloc(ATYPE_INT, 20); // slot 1, arena [10..30)
        heap.store(0, 5, 55);
        heap.store(1, 15, 1515);
        assert_eq!(heap.load(0, 5), Some(55));
        assert_eq!(heap.load(1, 15), Some(1515));
        // Verify arena contains both arrays' data
        assert_eq!(heap.arena.len(), 30);
    }

    #[test]
    fn arena_compaction_reclaims_space() {
        let mut heap = ArrayHeap::new();
        let mut buf = Vec::new();
        // Allocate 3 arena-backed arrays of 10 elements each
        heap.alloc(ATYPE_INT, 10); // slot 0
        heap.alloc(ATYPE_INT, 10); // slot 1
        heap.alloc(ATYPE_INT, 10); // slot 2
                                   // Write sentinel values
        heap.store(0, 0, 111);
        heap.store(1, 0, 222);
        heap.store(2, 0, 333);
        assert_eq!(heap.arena.len(), 30);

        // Free the middle array
        heap.free(1);
        heap.compact_arena(&mut buf);

        // Arena should shrink: 2 live arrays * 10 = 20
        assert_eq!(heap.arena.len(), 20);
        // Surviving data intact
        assert_eq!(heap.load(0, 0), Some(111));
        assert_eq!(heap.load(2, 0), Some(333));
    }

    #[test]
    fn arena_compaction_updates_offsets() {
        let mut heap = ArrayHeap::new();
        let mut buf = Vec::new();
        heap.alloc(ATYPE_INT, 10); // slot 0
        heap.alloc(ATYPE_INT, 10); // slot 1
        heap.alloc(ATYPE_INT, 10); // slot 2
                                   // Fill each with distinct pattern
        for i in 0..10 {
            heap.store(0, i, 100 + i as i32);
            heap.store(1, i, 200 + i as i32);
            heap.store(2, i, 300 + i as i32);
        }
        // Free first array, compact
        heap.free(0);
        heap.compact_arena(&mut buf);

        assert_eq!(heap.arena.len(), 20);
        // Array at slot 1 should now start at offset 0
        for i in 0..10 {
            assert_eq!(heap.load(1, i), Some(200 + i as i32));
        }
        // Array at slot 2 should start at offset 10
        for i in 0..10 {
            assert_eq!(heap.load(2, i), Some(300 + i as i32));
        }
    }

    #[test]
    fn arena_alloc_after_compact_reuses_space() {
        let mut heap = ArrayHeap::new();
        let mut buf = Vec::new();
        heap.alloc(ATYPE_INT, 10); // slot 0
        heap.alloc(ATYPE_INT, 10); // slot 1
        heap.store(1, 0, 42);

        // Free all and compact
        heap.free(0);
        heap.free(1);
        heap.compact_arena(&mut buf);
        assert_eq!(heap.arena.len(), 0);

        // New allocation reuses slot 0 and appends to (now empty) arena
        assert_eq!(heap.alloc(ATYPE_INT, 10), Some(0));
        heap.store(0, 5, 99);
        assert_eq!(heap.load(0, 5), Some(99));
        assert_eq!(heap.arena.len(), 10);
    }

    #[test]
    fn arena_mixed_inline_and_arena() {
        let mut heap = ArrayHeap::new();
        let mut buf = Vec::new();
        heap.alloc(ATYPE_INT, 4); // slot 0, inline
        heap.alloc(ATYPE_INT, 20); // slot 1, arena
        heap.alloc(ATYPE_INT, 2); // slot 2, inline
        heap.alloc(ATYPE_INT, 15); // slot 3, arena
        heap.store(0, 0, 1);
        heap.store(1, 10, 2);
        heap.store(2, 0, 3);
        heap.store(3, 10, 4);

        // Free one arena array, compact
        heap.free(1);
        heap.compact_arena(&mut buf);

        // Inline arrays unaffected
        assert_eq!(heap.load(0, 0), Some(1));
        assert_eq!(heap.load(2, 0), Some(3));
        // Surviving arena array intact
        assert_eq!(heap.load(3, 10), Some(4));
        // Arena shrunk to just the one live arena array
        assert_eq!(heap.arena.len(), 15);
    }
}
