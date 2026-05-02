// SPDX-License-Identifier: GPL-3.0-only
//! Pure USB CDC descriptors and byte-assembly helpers.
//!
//! No `rp-pico`/`rp235x-hal`/FreeRTOS deps — host-compilable and unit-testable.
//! The USB control-transfer state machine remains in `mod.rs` because it is
//! intrinsically coupled to the ISR and DPRAM register layout.

/// VID 0x1209 (pid.codes open-source), PID 0xCDC0 (picodroid CDC).
pub const DEVICE_DESC: [u8; 18] = [
    18, 0x01, 0x00, 0x02, 0x02, 0x00, 0x00, 64, 0x09, 0x12, 0xC0, 0xCD, 0x00, 0x01, 1, 2, 0, 1,
];

pub const CONFIG_DESC: [u8; 67] = [
    // Configuration
    9, 0x02, 67, 0, 2, 1, 0, 0x80, 250, // Interface 0: CDC Control (1 endpoint)
    9, 0x04, 0, 0, 1, 0x02, 0x02, 0x01, 0, // CDC Header FD
    5, 0x24, 0x00, 0x20, 0x01, // CDC Call Management FD
    5, 0x24, 0x01, 0x00, 0x01, // CDC ACM FD
    4, 0x24, 0x02, 0x02, // CDC Union FD
    5, 0x24, 0x06, 0x00, 0x01, // EP2 IN: interrupt, 8 bytes, 255ms
    7, 0x05, 0x82, 0x03, 8, 0, 255, // Interface 1: CDC Data (2 endpoints)
    9, 0x04, 1, 0, 2, 0x0A, 0x00, 0x00, 0, // EP1 OUT: bulk, 64 bytes
    7, 0x05, 0x01, 0x02, 64, 0, 0, // EP1 IN: bulk, 64 bytes
    7, 0x05, 0x81, 0x02, 64, 0, 0,
];

/// String descriptor 0: language (English US).
pub const STR0: [u8; 4] = [4, 0x03, 0x09, 0x04];

/// String 1: "Picodroid".
pub const STR1: [u8; 20] = [
    20, 0x03, b'P', 0, b'i', 0, b'c', 0, b'o', 0, b'd', 0, b'r', 0, b'o', 0, b'i', 0, b'd', 0,
];

/// String 2: "PDB (USB CDC)".
pub const STR2: [u8; 28] = [
    28, 0x03, b'P', 0, b'D', 0, b'B', 0, b' ', 0, b'(', 0, b'U', 0, b'S', 0, b'B', 0, b' ', 0,
    b'C', 0, b'D', 0, b'C', 0, b')', 0,
];

/// Line coding: 115200 8N1 (returned for GET_LINE_CODING).
pub const LINE_CODING: [u8; 7] = [0x00, 0xC2, 0x01, 0x00, 0x00, 0x00, 0x08];

/// Assemble a little-endian u32 from four bytes in arrival order.
pub fn assemble_u32_le(b0: u8, b1: u8, b2: u8, b3: u8) -> u32 {
    (b0 as u32) | ((b1 as u32) << 8) | ((b2 as u32) << 16) | ((b3 as u32) << 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_u32_le_zero() {
        assert_eq!(assemble_u32_le(0, 0, 0, 0), 0);
    }

    #[test]
    fn assemble_u32_le_max() {
        assert_eq!(assemble_u32_le(0xFF, 0xFF, 0xFF, 0xFF), 0xFFFF_FFFF);
    }

    #[test]
    fn assemble_u32_le_low_byte() {
        assert_eq!(assemble_u32_le(0x42, 0, 0, 0), 0x0000_0042);
    }

    #[test]
    fn assemble_u32_le_high_byte() {
        assert_eq!(assemble_u32_le(0, 0, 0, 0x42), 0x4200_0000);
    }

    #[test]
    fn assemble_u32_le_byte_order() {
        // 0xDEADBEEF little-endian: EF, BE, AD, DE arrive in that order.
        assert_eq!(assemble_u32_le(0xEF, 0xBE, 0xAD, 0xDE), 0xDEAD_BEEF);
    }

    #[test]
    fn assemble_u32_le_distinct_bytes() {
        assert_eq!(assemble_u32_le(0x01, 0x02, 0x03, 0x04), 0x0403_0201);
    }

    // ── Descriptor layout invariants ──────────────────────────────────────

    #[test]
    fn device_desc_len_matches_bLength() {
        assert_eq!(DEVICE_DESC.len() as u8, DEVICE_DESC[0]);
    }

    #[test]
    fn device_desc_type_is_DEVICE() {
        // USB descriptor type: DEVICE = 0x01.
        assert_eq!(DEVICE_DESC[1], 0x01);
    }

    #[test]
    fn device_desc_vid_pid() {
        // Bytes 8..10 = idVendor LE, 10..12 = idProduct LE.
        let vid = u16::from_le_bytes([DEVICE_DESC[8], DEVICE_DESC[9]]);
        let pid = u16::from_le_bytes([DEVICE_DESC[10], DEVICE_DESC[11]]);
        assert_eq!(vid, 0x1209);
        assert_eq!(pid, 0xCDC0);
    }

    #[test]
    fn config_desc_wTotalLength_matches_buffer() {
        // Bytes 2..4 = wTotalLength LE; must equal the buffer length.
        let total = u16::from_le_bytes([CONFIG_DESC[2], CONFIG_DESC[3]]);
        assert_eq!(total as usize, CONFIG_DESC.len());
    }

    #[test]
    fn config_desc_two_interfaces() {
        // bNumInterfaces at offset 4.
        assert_eq!(CONFIG_DESC[4], 2);
    }

    #[test]
    fn str0_is_lang_english_us() {
        assert_eq!(STR0[0], 4); // bLength
        assert_eq!(STR0[1], 0x03); // STRING descriptor
        assert_eq!(u16::from_le_bytes([STR0[2], STR0[3]]), 0x0409); // en-US
    }

    #[test]
    fn str1_encodes_picodroid() {
        assert_eq!(STR1[0] as usize, STR1.len());
        assert_eq!(STR1[1], 0x03);
        let decoded: alloc::string::String = STR1[2..].chunks(2).map(|c| c[0] as char).collect();
        assert_eq!(decoded, "Picodroid");
    }

    #[test]
    fn line_coding_115200_8N1() {
        // Bytes 0..4 = baud LE (115200 = 0x0001C200), 4 = stop (0), 5 = parity (0), 6 = data bits (8).
        let baud = u32::from_le_bytes([
            LINE_CODING[0],
            LINE_CODING[1],
            LINE_CODING[2],
            LINE_CODING[3],
        ]);
        assert_eq!(baud, 115_200);
        assert_eq!(LINE_CODING[4], 0); // 1 stop bit
        assert_eq!(LINE_CODING[5], 0); // no parity
        assert_eq!(LINE_CODING[6], 8); // 8 data bits
    }
}
