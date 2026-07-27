// SPDX-License-Identifier: GPL-3.0-only
//! Streaming IEEE CRC-32 over a 16-entry (64-byte) nibble table.
//!
//! Replaces the `crc32fast` dependency on both ends of the wire. In firmware
//! that crate's slicing-by-16 implementation carries a 16 KiB lookup table in
//! `.rodata` — ~1.8% of the RP2040's 896K program region — buying throughput
//! this protocol never needs (PDB frames are small and PAPK install
//! verification is a one-shot pass). The nibble table costs ~20 cycles/byte
//! on an M0+.
//!
//! The host `pdb` CLI now uses this same implementation rather than its own
//! hasher: the property that matters is that both ends produce identical
//! bytes, and sharing the code makes that true by construction instead of by
//! two independent implementations of the same standard. `known_vectors`
//! still pins it to IEEE CRC-32 (zlib, `crc32fast`, …) so the format stays
//! interoperable with anything else that speaks it.

const POLY: u32 = 0xEDB8_8320; // IEEE, reflected

const TABLE: [u32; 16] = build_table();

const fn build_table() -> [u32; 16] {
    let mut table = [0u32; 16];
    let mut i = 0;
    while i < 16 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 4 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// Incremental IEEE CRC-32 hasher (drop-in for `crc32fast::Hasher`:
/// `new` → `update`* → `finalize`).
pub struct Crc32 {
    state: u32,
}

impl Crc32 {
    pub fn new() -> Self {
        Self { state: 0xFFFF_FFFF }
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut s = self.state;
        for &byte in data {
            s ^= byte as u32;
            s = (s >> 4) ^ TABLE[(s & 0xF) as usize];
            s = (s >> 4) ^ TABLE[(s & 0xF) as usize];
        }
        self.state = s;
    }

    pub fn finalize(self) -> u32 {
        !self.state
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Crc32;

    fn crc(data: &[u8]) -> u32 {
        let mut h = Crc32::new();
        h.update(data);
        h.finalize()
    }

    #[test]
    fn known_vectors() {
        // The CRC-32/ISO-HDLC check value and friends — any IEEE CRC-32
        // implementation (crc32fast, zlib) produces exactly these.
        assert_eq!(crc(b""), 0);
        assert_eq!(crc(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
        assert_eq!(crc(&[0u8; 32]), 0x190A_55AD);
        assert_eq!(crc(&[0xFFu8; 32]), 0xFF6C_AB0B);
    }

    #[test]
    fn streaming_equals_one_shot() {
        let data = b"cmd\x00\x10\x00\x00payload bytes here";
        let mut h = Crc32::new();
        for chunk in data.chunks(3) {
            h.update(chunk);
        }
        assert_eq!(h.finalize(), crc(data));
    }
}
