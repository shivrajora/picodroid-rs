// SPDX-License-Identifier: GPL-3.0-only
//! An aligned copy of a byte buffer, for the host.

use alloc::vec::Vec;

/// A copy of a PAPK (or any byte buffer) at an 8-byte aligned address.
///
/// The class section is read in place as `u16` words and 8-byte index
/// entries, which a flash image or a `.rodata` static satisfies; a host
/// `Vec<u8>` or `include_bytes!` data promises no alignment at all. Host
/// tools and tests copy into one of these before parsing
/// (`Papk::parse(&buf)`).
pub struct AlignedBuf {
    words: Vec<u64>,
    len: usize,
}

impl AlignedBuf {
    pub fn new(bytes: &[u8]) -> Self {
        let mut words = alloc::vec![0u64; bytes.len().div_ceil(8)];
        // SAFETY: `words` holds at least `bytes.len()` bytes; a `u8` view
        // of `u64` storage, written once here.
        let dst = unsafe {
            core::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), bytes.len())
        };
        dst.copy_from_slice(bytes);
        Self {
            words,
            len: bytes.len(),
        }
    }
}

impl core::ops::Deref for AlignedBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        // SAFETY: `words` holds at least `len` bytes, all initialised.
        unsafe { core::slice::from_raw_parts(self.words.as_ptr().cast::<u8>(), self.len) }
    }
}
