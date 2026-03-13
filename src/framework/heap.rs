/// JVM string table for Milestone 1.
///
/// All strings in M1 are UTF-8 literals baked into .class files in Flash.
/// We store (ptr, len) pairs pointing directly into Flash — zero allocation.
use heapless::Vec;

pub struct StringTable {
    ptrs: Vec<*const u8, 32>,
    lens: Vec<u16, 32>,
}

// SAFETY: the pointers reference static Flash data which is never mutated.
unsafe impl Send for StringTable {}

impl StringTable {
    pub const fn new() -> Self {
        Self {
            ptrs: Vec::new(),
            lens: Vec::new(),
        }
    }

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
        self.ptrs.push(s.as_ptr()).ok()?;
        self.lens.push(s.len() as u16).ok()?;
        Some(idx)
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
    fn intern_fills_to_capacity() {
        static S00: &[u8] = b"s00";
        static S01: &[u8] = b"s01";
        static S02: &[u8] = b"s02";
        static S03: &[u8] = b"s03";
        static S04: &[u8] = b"s04";
        static S05: &[u8] = b"s05";
        static S06: &[u8] = b"s06";
        static S07: &[u8] = b"s07";
        static S08: &[u8] = b"s08";
        static S09: &[u8] = b"s09";
        static S10: &[u8] = b"s10";
        static S11: &[u8] = b"s11";
        static S12: &[u8] = b"s12";
        static S13: &[u8] = b"s13";
        static S14: &[u8] = b"s14";
        static S15: &[u8] = b"s15";
        static S16: &[u8] = b"s16";
        static S17: &[u8] = b"s17";
        static S18: &[u8] = b"s18";
        static S19: &[u8] = b"s19";
        static S20: &[u8] = b"s20";
        static S21: &[u8] = b"s21";
        static S22: &[u8] = b"s22";
        static S23: &[u8] = b"s23";
        static S24: &[u8] = b"s24";
        static S25: &[u8] = b"s25";
        static S26: &[u8] = b"s26";
        static S27: &[u8] = b"s27";
        static S28: &[u8] = b"s28";
        static S29: &[u8] = b"s29";
        static S30: &[u8] = b"s30";
        static S31: &[u8] = b"s31";
        static S32: &[u8] = b"s32"; // one beyond capacity

        let slots: [&'static [u8]; 32] = [
            S00, S01, S02, S03, S04, S05, S06, S07, S08, S09, S10, S11, S12, S13, S14, S15, S16,
            S17, S18, S19, S20, S21, S22, S23, S24, S25, S26, S27, S28, S29, S30, S31,
        ];

        let mut table = StringTable::new();
        for (expected_idx, &slot) in slots.iter().enumerate() {
            let result = table.intern(slot);
            assert_eq!(
                result,
                Some(expected_idx as u16),
                "slot {expected_idx} should intern successfully"
            );
        }

        // Table is now full; the 33rd intern must return None.
        assert_eq!(table.intern(S32), None);
    }
}
