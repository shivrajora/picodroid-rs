/// JVM string table for Milestone 1.
///
/// All strings in M1 are UTF-8 literals baked into .class files in Flash.
/// We store (ptr, len) pairs pointing directly into Flash — zero allocation.
use alloc::vec::Vec;

pub struct StringTable {
    ptrs: Vec<*const u8>,
    lens: Vec<u16>,
    dyn_slot: Option<u16>,
}

// SAFETY: the pointers reference static Flash data which is never mutated.
unsafe impl Send for StringTable {}

impl StringTable {
    pub const fn new() -> Self {
        Self {
            ptrs: Vec::new(),
            lens: Vec::new(),
            dyn_slot: None,
        }
    }
}

impl Default for StringTable {
    fn default() -> Self {
        Self::new()
    }
}

impl StringTable {
    /// Intern a UTF-8 byte slice (must have 'static lifetime, e.g. from Flash).
    /// Returns the Reference index (u16).
    pub fn intern(&mut self, s: &'static [u8]) -> Option<u16> {
        // Check for existing entry
        for i in 0..self.ptrs.len() {
            let existing =
                unsafe { core::slice::from_raw_parts(self.ptrs[i], self.lens[i] as usize) };
            if existing == s {
                return Some(i as u16);
            }
        }
        let idx = self.ptrs.len() as u16;
        self.ptrs.push(s.as_ptr());
        self.lens.push(s.len() as u16);
        Some(idx)
    }

    /// Intern a dynamically-built string, reusing a single slot to avoid exhaustion.
    ///
    /// # Safety
    /// `ptr` must remain valid (pointing into stable memory, e.g. `ObjectHeap::sb_buf`)
    /// until the next call to `intern_dyn`, which overwrites this slot.
    /// The caller must not mutate the backing buffer between calling this function
    /// and consuming the returned index (e.g. passing to the operand stack).
    pub unsafe fn intern_dyn(&mut self, ptr: *const u8, len: usize) -> Option<u16> {
        match self.dyn_slot {
            Some(slot) => {
                let i = slot as usize;
                self.ptrs[i] = ptr;
                self.lens[i] = len as u16;
                Some(slot)
            }
            None => {
                let idx = self.ptrs.len() as u16;
                self.ptrs.push(ptr);
                self.lens.push(len as u16);
                self.dyn_slot = Some(idx);
                Some(idx)
            }
        }
    }

    /// Resolve a Reference index to a `&'static str`.
    /// SAFETY: all stored pointers reference static Flash data (from `include_bytes!`).
    pub fn resolve(&self, idx: u16) -> Option<&'static str> {
        let i = idx as usize;
        if i >= self.ptrs.len() {
            return None;
        }
        let slice = unsafe { core::slice::from_raw_parts(self.ptrs[i], self.lens[i] as usize) };
        core::str::from_utf8(slice).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static HELLO: &[u8] = b"hello";
    static WORLD: &[u8] = b"world";
    static FOO: &[u8] = b"foo";

    #[test]
    fn intern_returns_sequential_indices() {
        let mut table = StringTable::new();
        let idx0 = table.intern(HELLO);
        let idx1 = table.intern(WORLD);
        let idx2 = table.intern(FOO);
        assert_eq!(idx0, Some(0));
        assert_eq!(idx1, Some(1));
        assert_eq!(idx2, Some(2));
    }

    #[test]
    fn intern_deduplicates() {
        let mut table = StringTable::new();
        let idx_first = table.intern(HELLO);
        let idx_second = table.intern(HELLO);
        assert!(idx_first.is_some());
        assert_eq!(idx_first, idx_second);
    }

    #[test]
    fn resolve_returns_correct_str() {
        let mut table = StringTable::new();
        let idx = table.intern(HELLO).expect("intern should succeed");
        let resolved = table.resolve(idx);
        assert_eq!(resolved, Some("hello"));
    }

    #[test]
    fn resolve_out_of_bounds() {
        let table = StringTable::new();
        assert_eq!(table.resolve(99), None);
    }

    #[test]
    fn intern_beyond_old_capacity_succeeds() {
        // Previously capped at 32; verify we can now go well beyond that.
        let strs: [&'static [u8]; 33] = [
            b"s00", b"s01", b"s02", b"s03", b"s04", b"s05", b"s06", b"s07", b"s08", b"s09", b"s10",
            b"s11", b"s12", b"s13", b"s14", b"s15", b"s16", b"s17", b"s18", b"s19", b"s20", b"s21",
            b"s22", b"s23", b"s24", b"s25", b"s26", b"s27", b"s28", b"s29", b"s30", b"s31", b"s32",
        ];
        let mut table = StringTable::new();
        for (expected_idx, &s) in strs.iter().enumerate() {
            let result = table.intern(s);
            assert_eq!(
                result,
                Some(expected_idx as u16),
                "slot {expected_idx} should intern successfully"
            );
        }
    }
}
