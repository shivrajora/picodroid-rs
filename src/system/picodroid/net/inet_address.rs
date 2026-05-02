// SPDX-License-Identifier: GPL-3.0-only
//! Native implementations for picodroid.net.InetAddress.

use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::helpers::extract_obj_idx;

/// InetAddress.getHostAddress() -> String
///
/// Builds "a.b.c.d" in Rust to avoid the shared-StringBuilder problem
/// that occurs when Java string concatenation is nested.
pub fn get_host_address_native(
    args: &[Value],
    objects: &ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let addr = match objects.get_field(idx, 0) {
        Some(Value::Int(v)) => v as u32,
        _ => return Err(JvmError::InvalidReference),
    };

    let a = (addr >> 24) & 0xFF;
    let b = (addr >> 16) & 0xFF;
    let c = (addr >> 8) & 0xFF;
    let d = addr & 0xFF;

    // Format into a stack buffer — "255.255.255.255" is at most 15 bytes.
    let mut buf = [0u8; 16];
    let mut pos = 0;

    for (i, octet) in [a, b, c, d].iter().enumerate() {
        if i > 0 {
            buf[pos] = b'.';
            pos += 1;
        }
        pos += write_u32(&mut buf[pos..], *octet);
    }

    let str_ref = strings
        .intern_dyn(&buf[..pos])
        .ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::Reference(str_ref)))
}

/// Write a u32 (0..255) as decimal ASCII into `buf`, return bytes written.
fn write_u32(buf: &mut [u8], val: u32) -> usize {
    if val >= 100 {
        buf[0] = b'0' + (val / 100) as u8;
        buf[1] = b'0' + ((val / 10) % 10) as u8;
        buf[2] = b'0' + (val % 10) as u8;
        3
    } else if val >= 10 {
        buf[0] = b'0' + (val / 10) as u8;
        buf[1] = b'0' + (val % 10) as u8;
        2
    } else {
        buf[0] = b'0' + val as u8;
        1
    }
}
