/// JVM string table.
///
/// Static strings (UTF-8 literals baked into .class files in Flash) are stored
/// as (ptr, len) pairs pointing directly into Flash — zero allocation.
///
/// Dynamic strings (produced at runtime, e.g. by StringBuilder.toString() or
/// String methods like substring/trim) are owned by this table via `dyn_bufs`.
/// Each dynamic entry is `Some(Vec<u8>)`; when GC is implemented it can free
/// individual entries by calling [`StringTable::free_dyn`], and the slot will
/// be reused on the next [`StringTable::intern_dyn`] call.
///
/// # Layout invariant
/// Static entries (added via [`intern`]) are expected to be added before any
/// dynamic entries.  In practice all `.class` loading happens at startup before
/// runtime string operations.
use alloc::vec::Vec;

pub struct StringTable {
    ptrs: Vec<*const u8>,
    lens: Vec<u16>,
    /// Backing storage for dynamic strings.  `dyn_bufs[i]` corresponds to
    /// `ptrs[dyn_start + i]`.  `None` means the slot has been freed (by GC)
    /// and is available for reuse.
    dyn_bufs: Vec<Option<Vec<u8>>>,
    /// Index into `ptrs`/`lens` where the first dynamic entry lives.
    dyn_start: usize,
}

// SAFETY: the static pointers reference Flash data which is never mutated.
// Dynamic pointers point into heap-allocated `Vec<u8>` data that is pinned
// (moving the Vec struct moves only the fat pointer, not the heap bytes).
unsafe impl Send for StringTable {}

impl StringTable {
    pub const fn new() -> Self {
        Self {
            ptrs: Vec::new(),
            lens: Vec::new(),
            dyn_bufs: Vec::new(),
            dyn_start: 0,
        }
    }
}

impl Default for StringTable {
    fn default() -> Self {
        Self::new()
    }
}

impl StringTable {
    /// Intern a UTF-8 byte slice (must have `'static` lifetime, e.g. from Flash).
    /// Returns the Reference index (u16).  Deduplicates identical static strings.
    pub fn intern(&mut self, s: &'static [u8]) -> Option<u16> {
        // Deduplicate against existing static entries only.
        for i in 0..self.dyn_start {
            let existing =
                unsafe { core::slice::from_raw_parts(self.ptrs[i], self.lens[i] as usize) };
            if existing == s {
                return Some(i as u16);
            }
        }
        let idx = self.ptrs.len() as u16;
        self.ptrs.push(s.as_ptr());
        self.lens.push(s.len() as u16);
        // Advance dyn_start to keep it past all static entries.
        self.dyn_start = self.ptrs.len();
        Some(idx)
    }

    /// Intern a dynamically-built string by copying `bytes` into owned storage.
    ///
    /// Reuses a freed (GC'd) slot when available; otherwise grows the table.
    /// Safe — this table owns the backing `Vec<u8>` for each dynamic entry.
    pub fn intern_dyn(&mut self, bytes: &[u8]) -> Option<u16> {
        // Look for a freed slot to reuse.
        for (di, slot) in self.dyn_bufs.iter_mut().enumerate() {
            if slot.is_none() {
                let idx = (self.dyn_start + di) as u16;
                let buf: Vec<u8> = bytes.to_vec();
                // SAFETY: buf is moved into `*slot` below; the heap allocation
                // for the bytes does not move when the Vec struct is moved.
                self.ptrs[idx as usize] = buf.as_ptr();
                self.lens[idx as usize] = buf.len() as u16;
                *slot = Some(buf);
                return Some(idx);
            }
        }
        // No free slot — append a new entry.
        let buf: Vec<u8> = bytes.to_vec();
        let idx = self.ptrs.len() as u16;
        // SAFETY: same as above — heap allocation stays fixed when Vec moves.
        self.ptrs.push(buf.as_ptr());
        self.lens.push(buf.len() as u16);
        self.dyn_bufs.push(Some(buf));
        Some(idx)
    }

    /// Free a dynamic string slot (for GC use).
    ///
    /// If `idx` does not refer to a dynamic entry, this is a no-op.
    /// The freed slot will be reused by the next [`intern_dyn`] call.
    pub fn free_dyn(&mut self, idx: u16) {
        let i = idx as usize;
        if i < self.dyn_start || i >= self.ptrs.len() {
            return;
        }
        let di = i - self.dyn_start;
        if di < self.dyn_bufs.len() {
            self.dyn_bufs[di] = None;
            self.ptrs[i] = core::ptr::null();
            self.lens[i] = 0;
        }
    }

    // ── GC support ────────────────────────────────────────────────────────────

    /// Index where dynamic entries begin (entries before this are static/Flash).
    pub fn dyn_start(&self) -> usize {
        self.dyn_start
    }

    /// Total number of entries (static + dynamic).
    pub fn total_len(&self) -> usize {
        self.ptrs.len()
    }

    /// Returns `true` if `idx` is a dynamic entry that is currently live.
    pub fn is_dyn_live(&self, idx: u16) -> bool {
        let i = idx as usize;
        if i < self.dyn_start || i >= self.ptrs.len() {
            return false;
        }
        let di = i - self.dyn_start;
        di < self.dyn_bufs.len() && self.dyn_bufs[di].is_some()
    }

    /// Resolve a Reference index to a `&str`.
    pub fn resolve(&self, idx: u16) -> Option<&str> {
        let i = idx as usize;
        if i >= self.ptrs.len() {
            return None;
        }
        let ptr = self.ptrs[i];
        if ptr.is_null() {
            return None;
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, self.lens[i] as usize) };
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

    #[test]
    fn intern_dyn_stores_and_resolves() {
        let mut table = StringTable::new();
        let idx = table
            .intern_dyn(b"dynamic")
            .expect("intern_dyn should succeed");
        assert_eq!(table.resolve(idx), Some("dynamic"));
    }

    #[test]
    fn intern_dyn_multiple_slots() {
        let mut table = StringTable::new();
        let a = table.intern_dyn(b"aaa").unwrap();
        let b = table.intern_dyn(b"bbb").unwrap();
        assert_ne!(a, b);
        assert_eq!(table.resolve(a), Some("aaa"));
        assert_eq!(table.resolve(b), Some("bbb"));
    }

    #[test]
    fn intern_dyn_reuses_freed_slot() {
        let mut table = StringTable::new();
        let idx = table.intern_dyn(b"first").unwrap();
        // Simulate GC freeing the slot.
        table.free_dyn(idx);
        assert_eq!(table.resolve(idx), None);
        // Next intern_dyn should reuse the slot.
        let idx2 = table.intern_dyn(b"second").unwrap();
        assert_eq!(idx, idx2);
        assert_eq!(table.resolve(idx2), Some("second"));
    }

    #[test]
    fn static_and_dyn_coexist() {
        let mut table = StringTable::new();
        let si = table.intern(HELLO).unwrap();
        let di = table.intern_dyn(b"runtime").unwrap();
        assert_eq!(table.resolve(si), Some("hello"));
        assert_eq!(table.resolve(di), Some("runtime"));
    }

    #[test]
    fn free_dyn_static_entry_is_noop() {
        let mut table = StringTable::new();
        let si = table.intern(HELLO).unwrap();
        table.free_dyn(si); // should not crash or affect static entry
        assert_eq!(table.resolve(si), Some("hello"));
    }
}
