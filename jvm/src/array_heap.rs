use alloc::vec;
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

/// Array data stored either inline (small arrays) or on the heap (large arrays).
/// Using an enum avoids paying for the inline buffer when the heap path is used,
/// keeping slot size small for large-array workloads like `int[100]`.
enum ArrayData {
    Inline { buf: [i32; INLINE_DATA], len: u16 },
    Heap(Vec<i32>),
}

pub struct JvmArray {
    pub atype: u8,
    data: ArrayData,
}

impl JvmArray {
    fn new(atype: u8, len: u16) -> Self {
        if (len as usize) <= INLINE_DATA {
            Self {
                atype,
                data: ArrayData::Inline {
                    buf: [0i32; INLINE_DATA],
                    len,
                },
            }
        } else {
            Self {
                atype,
                data: ArrayData::Heap(vec![0i32; len as usize]),
            }
        }
    }

    fn len(&self) -> u16 {
        match &self.data {
            ArrayData::Inline { len, .. } => *len,
            ArrayData::Heap(v) => v.len() as u16,
        }
    }

    fn load(&self, elem: usize) -> Option<i32> {
        match &self.data {
            ArrayData::Inline { buf, len } => {
                if elem >= *len as usize {
                    return None;
                }
                Some(buf[elem])
            }
            ArrayData::Heap(v) => v.get(elem).copied(),
        }
    }

    fn store(&mut self, elem: usize, val: i32) -> Option<()> {
        match &mut self.data {
            ArrayData::Inline { buf, len } => {
                if elem >= *len as usize {
                    return None;
                }
                buf[elem] = val;
                Some(())
            }
            ArrayData::Heap(v) => {
                if elem >= v.len() {
                    return None;
                }
                v[elem] = val;
                Some(())
            }
        }
    }

    fn data_slice(&self) -> &[i32] {
        match &self.data {
            ArrayData::Inline { buf, len } => &buf[..*len as usize],
            ArrayData::Heap(v) => v.as_slice(),
        }
    }
}

pub struct ArrayHeap {
    arrays: Vec<Option<JvmArray>>,
    /// Lowest index that might contain a `None` slot; avoids O(n) scans.
    first_free: usize,
}

impl ArrayHeap {
    pub const fn new() -> Self {
        Self {
            arrays: Vec::new(),
            first_free: 0,
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
    pub fn alloc(&mut self, atype: u8, len: u16) -> Option<u16> {
        let new_arr = JvmArray::new(atype, len);
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

    /// Load element at index elem from array idx. Returns None on out-of-bounds or invalid idx.
    pub fn load(&self, idx: u16, elem: usize) -> Option<i32> {
        self.arrays.get(idx as usize)?.as_ref()?.load(elem)
    }

    /// Store value at index elem in array idx. Returns None on out-of-bounds or invalid idx.
    pub fn store(&mut self, idx: u16, elem: usize, val: i32) -> Option<()> {
        self.arrays
            .get_mut(idx as usize)?
            .as_mut()?
            .store(elem, val)
    }

    /// Return the length of array idx.
    pub fn length(&self, idx: u16) -> Option<u16> {
        Some(self.arrays.get(idx as usize)?.as_ref()?.len())
    }

    #[allow(dead_code)]
    pub fn atype(&self, idx: u16) -> Option<u8> {
        Some(self.arrays.get(idx as usize)?.as_ref()?.atype)
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
        self.arrays
            .get(idx as usize)
            .and_then(|a| a.as_ref())
            .map(|a| a.data_slice())
            .unwrap_or(&[])
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
}
