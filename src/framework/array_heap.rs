use heapless::Vec;

pub const MAX_ARRAY_ELEMENTS: usize = 16;
pub const MAX_ARRAYS: usize = 8;

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

pub struct JvmArray {
    pub atype: u8,
    pub len: u8,
    pub data: [i32; MAX_ARRAY_ELEMENTS],
}

pub struct ArrayHeap {
    arrays: Vec<JvmArray, MAX_ARRAYS>,
}

impl ArrayHeap {
    pub const fn new() -> Self {
        Self { arrays: Vec::new() }
    }

    /// Allocate a new array. Returns its heap index, or None if heap is full or len exceeds capacity.
    pub fn alloc(&mut self, atype: u8, len: u8) -> Option<u16> {
        if len as usize > MAX_ARRAY_ELEMENTS {
            return None;
        }
        let idx = self.arrays.len() as u16;
        self.arrays
            .push(JvmArray {
                atype,
                len,
                data: [0i32; MAX_ARRAY_ELEMENTS],
            })
            .ok()?;
        Some(idx)
    }

    /// Load element at index elem from array idx. Returns None on out-of-bounds or invalid idx.
    pub fn load(&self, idx: u16, elem: usize) -> Option<i32> {
        let arr = self.arrays.get(idx as usize)?;
        if elem >= arr.len as usize {
            return None;
        }
        Some(arr.data[elem])
    }

    /// Store value at index elem in array idx. Returns None on out-of-bounds or invalid idx.
    pub fn store(&mut self, idx: u16, elem: usize, val: i32) -> Option<()> {
        let arr = self.arrays.get_mut(idx as usize)?;
        if elem >= arr.len as usize {
            return None;
        }
        arr.data[elem] = val;
        Some(())
    }

    /// Return the length of array idx.
    pub fn length(&self, idx: u16) -> Option<u8> {
        Some(self.arrays.get(idx as usize)?.len)
    }

    #[allow(dead_code)]
    pub fn atype(&self, idx: u16) -> Option<u8> {
        Some(self.arrays.get(idx as usize)?.atype)
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
    fn alloc_full_returns_none() {
        let mut heap = ArrayHeap::new();
        for _ in 0..MAX_ARRAYS {
            assert!(heap.alloc(ATYPE_INT, 1).is_some());
        }
        assert_eq!(heap.alloc(ATYPE_INT, 1), None);
    }

    #[test]
    fn alloc_exceeds_max_elements_returns_none() {
        let mut heap = ArrayHeap::new();
        assert_eq!(heap.alloc(ATYPE_INT, (MAX_ARRAY_ELEMENTS + 1) as u8), None);
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
}
