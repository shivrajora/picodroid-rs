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
    /// Seven bytes longer than the copy, so an 8-aligned window of `len`
    /// bytes fits wherever the allocator put it.
    bytes: Vec<u8>,
    start: usize,
    len: usize,
}

/// How far past `ptr` the next 8-byte boundary is.
fn pad_to_8(ptr: *const u8) -> usize {
    (8 - (ptr as usize % 8)) % 8
}

impl AlignedBuf {
    pub fn new(bytes: &[u8]) -> Self {
        // Never resized after this, so the window stays where it is.
        let mut copy = alloc::vec![0u8; bytes.len() + 7];
        let start = pad_to_8(copy.as_ptr());
        copy[start..start + bytes.len()].copy_from_slice(bytes);
        Self {
            bytes: copy,
            start,
            len: bytes.len(),
        }
    }
}

impl core::ops::Deref for AlignedBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes[self.start..self.start + self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_is_aligned_and_equal() {
        for len in 0..40usize {
            let src: Vec<u8> = (0..len as u8).collect();
            // Offset sources, so the copies land on varied allocations.
            let buf = AlignedBuf::new(&src);
            assert_eq!(&*buf, &src[..]);
            assert_eq!(buf.as_ptr() as usize % 8, 0);
            let moved = buf;
            assert_eq!(moved.as_ptr() as usize % 8, 0);
            assert_eq!(&*moved, &src[..]);
        }
    }
}
