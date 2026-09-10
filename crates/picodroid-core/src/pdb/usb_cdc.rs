// SPDX-License-Identifier: GPL-3.0-only
//! The reference USB CDC-ACM descriptor set for the debug bridge.
//!
//! The identity — vendor and product ID, the strings — is
//! [`pdb_protocol::usb`]'s, and every table here is built from it at compile
//! time, so the device and the host tool's port scan cannot disagree. What
//! this module adds is the *shape* of a CDC-ACM function as the RP family's
//! bare-metal driver presents it: one control interface with an interrupt
//! endpoint (EP2 IN, 8 bytes), one data interface with a bulk pair (EP1
//! OUT/IN, 64 bytes), full-speed.
//!
//! A family whose USB stack fixes different endpoint numbers or sizes builds
//! its own `CONFIG_DESC` — but must still take the identity from the protocol
//! crate, which is the part a host depends on.
//!
//! There is no simulator counterpart. The simulator ships no PDB *endpoint*
//! (no transport, no command loop serving a host); the whole bridge —
//! command loop, framing, install orchestration, sysmon, input — compiles on
//! the host in [`crate::pdb`] and [`crate::install`] and is tested against
//! mock transports. A real simulator endpoint (a `PdbTransport` over TCP or
//! a pty) was analysed and deferred in `docs/designs/pdb-schema-as-code.md`
//! §3, so it need not be re-derived.

use pdb_protocol::usb;

/// Device descriptor: USB 2.0, CDC class, 64-byte EP0, one configuration.
pub const DEVICE_DESC: [u8; 18] = device_descriptor(usb::VID, usb::PID);

/// Configuration descriptor: two interfaces (CDC control + CDC data),
/// bus-powered, 500 mA.
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

/// String 1: the manufacturer / product name.
pub const STR1: [u8; 20] = string_descriptor(usb::MANUFACTURER);

/// String 2: the interface name.
pub const STR2: [u8; 28] = string_descriptor(usb::INTERFACE);

/// Line coding: 115200 8N1 (returned for GET_LINE_CODING).
pub const LINE_CODING: [u8; 7] = [0x00, 0xC2, 0x01, 0x00, 0x00, 0x00, 0x08];

const fn device_descriptor(vid: u16, pid: u16) -> [u8; 18] {
    let [vid_lo, vid_hi] = vid.to_le_bytes();
    let [pid_lo, pid_hi] = pid.to_le_bytes();
    [
        18,     // bLength
        0x01,   // bDescriptorType: DEVICE
        0x00,   // bcdUSB 2.00
        0x02,   //
        0x02,   // bDeviceClass: CDC
        0x00,   // bDeviceSubClass
        0x00,   // bDeviceProtocol
        64,     // bMaxPacketSize0
        vid_lo, // idVendor
        vid_hi, //
        pid_lo, // idProduct
        pid_hi, //
        0x00,   // bcdDevice 1.00
        0x01,   //
        1,      // iManufacturer
        2,      // iProduct
        0,      // iSerialNumber
        1,      // bNumConfigurations
    ]
}

/// Encode an ASCII string as a USB string descriptor (UTF-16LE). The length is
/// checked at compile time, so a renamed string that no longer fits its table
/// fails the build rather than truncating on the wire.
const fn string_descriptor<const N: usize>(s: &str) -> [u8; N] {
    let bytes = s.as_bytes();
    assert!(
        2 + 2 * bytes.len() == N,
        "string descriptor table must be exactly 2 + 2 * chars long"
    );
    let mut out = [0u8; N];
    out[0] = N as u8;
    out[1] = 0x03; // STRING
    let mut i = 0;
    while i < bytes.len() {
        assert!(
            bytes[i] < 0x80,
            "USB strings here are ASCII encoded as UTF-16LE"
        );
        out[2 + 2 * i] = bytes[i];
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_string(desc: &[u8]) -> alloc::string::String {
        desc[2..].chunks(2).map(|c| c[0] as char).collect()
    }

    #[test]
    fn device_desc_len_matches_b_length() {
        assert_eq!(DEVICE_DESC.len() as u8, DEVICE_DESC[0]);
    }

    #[test]
    fn device_desc_type_is_device() {
        // USB descriptor type: DEVICE = 0x01.
        assert_eq!(DEVICE_DESC[1], 0x01);
    }

    #[test]
    fn device_desc_identity_is_the_protocol_crates() {
        // Bytes 8..10 = idVendor LE, 10..12 = idProduct LE. The host tool's
        // port scan reads the same two constants, so this is the test that
        // the two ends cannot drift.
        let vid = u16::from_le_bytes([DEVICE_DESC[8], DEVICE_DESC[9]]);
        let pid = u16::from_le_bytes([DEVICE_DESC[10], DEVICE_DESC[11]]);
        assert_eq!(vid, usb::VID);
        assert_eq!(pid, usb::PID);
        // And the numbers themselves, so a change to the protocol crate is a
        // deliberate change here too.
        assert_eq!((vid, pid), (0x1209, 0xCDC0));
    }

    #[test]
    fn config_desc_w_total_length_matches_buffer() {
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
    fn str1_encodes_the_manufacturer() {
        assert_eq!(STR1[0] as usize, STR1.len());
        assert_eq!(STR1[1], 0x03);
        assert_eq!(decode_string(&STR1), usb::MANUFACTURER);
        assert_eq!(decode_string(&STR1), "Picodroid");
    }

    #[test]
    fn str2_encodes_the_interface() {
        assert_eq!(STR2[0] as usize, STR2.len());
        assert_eq!(STR2[1], 0x03);
        assert_eq!(decode_string(&STR2), usb::INTERFACE);
        assert_eq!(decode_string(&STR2), "PDB (USB CDC)");
    }

    #[test]
    fn utf16_high_bytes_are_zero() {
        // Every odd byte after the header is the UTF-16 high byte: zero for
        // ASCII. A table built by hand with a shifted character would fail.
        for desc in [&STR1[..], &STR2[..]] {
            assert!(desc[2..].iter().skip(1).step_by(2).all(|&b| b == 0));
        }
    }

    #[test]
    fn line_coding_115200_8n1() {
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
