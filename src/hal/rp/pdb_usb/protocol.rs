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
