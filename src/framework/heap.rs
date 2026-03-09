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
