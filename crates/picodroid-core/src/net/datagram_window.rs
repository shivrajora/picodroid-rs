// SPDX-License-Identifier: GPL-3.0-only
//! The byte-marshalling half of `DatagramSocket.send`/`receive`: a packet's
//! buffer window as the natives read it, the datagram copied out of it and
//! back into it. Pure over the JVM heaps (no HAL), so it is compiled into
//! the host tests through a `#[path]` shim in `lib.rs` while `net` itself
//! is `cfg(not(test))`.

use alloc::vec::Vec;

use pico_jvm::array_heap::ArrayHeap;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

// `net` is `cfg(not(test))`; the host tests reach the slot table through
// its own shim.
#[cfg(not(test))]
use super::fields;
#[cfg(test)]
use crate::net_fields_tests as fields;

/// The largest UDP payload IPv4 carries (65535 minus the IP and UDP
/// headers). A packet declaring more is refused with `IOException`, as
/// `java.net` does, rather than sent truncated.
pub(crate) const MAX_DATAGRAM: usize = 65_507;

/// What a packet declares about its buffer window: the array, where the
/// datagram starts in it, how long it is, and the window's full size.
pub(crate) struct Window {
    pub(crate) arr: u16,
    pub(crate) offset: usize,
    pub(crate) length: usize,
    pub(crate) buf_length: usize,
}

pub(crate) fn packet_window(pkt_idx: u16, objects: &ObjectHeap) -> Result<Window, JvmError> {
    let arr = match objects.get_field(pkt_idx, fields::datagram_packet::DATA) {
        Some(Value::ArrayRef(idx)) => idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let int_field = |slot: usize| match objects.get_field(pkt_idx, slot) {
        Some(Value::Int(v)) if v >= 0 => Ok(v as usize),
        Some(Value::Int(_)) => Err(JvmError::ArrayIndexOutOfBounds),
        _ => Err(JvmError::InvalidReference),
    };
    Ok(Window {
        arr,
        offset: int_field(fields::datagram_packet::OFFSET)?,
        length: int_field(fields::datagram_packet::LENGTH)?,
        buf_length: int_field(fields::datagram_packet::BUF_LENGTH)?,
    })
}

/// The bytes a packet sends: its window `[offset, offset + length)` copied
/// out of the array, in a buffer sized to the datagram — the 256-byte stack
/// buffer that stood here sent anything longer truncated without a word.
/// A window outside the array is an `ArrayIndexOutOfBoundsException`; a
/// heap that cannot spare the buffer is the crate's allocation-failure
/// signal (the run loop's `OutOfMemoryError`).
pub(crate) fn payload_of(arrays: &ArrayHeap, w: &Window) -> Result<Vec<u8>, JvmError> {
    let arr_len = arrays.length(w.arr).ok_or(JvmError::InvalidReference)? as usize;
    let end = w
        .offset
        .checked_add(w.length)
        .filter(|&end| end <= arr_len)
        .ok_or(JvmError::ArrayIndexOutOfBounds)?;
    let mut buf: Vec<u8> = Vec::new();
    buf.try_reserve_exact(w.length)
        .map_err(|_| JvmError::StackOverflow)?;
    for i in w.offset..end {
        buf.push(
            arrays
                .load(w.arr, i)
                .ok_or(JvmError::ArrayIndexOutOfBounds)? as u8,
        );
    }
    Ok(buf)
}

/// Room for what a packet can receive: its window's full size, cut to what
/// the array holds past `offset` (the kernel discards the rest of a longer
/// datagram, as `java.net` documents).
pub(crate) fn receive_buffer(arrays: &ArrayHeap, w: &Window) -> Result<Vec<u8>, JvmError> {
    let arr_len = arrays.length(w.arr).ok_or(JvmError::InvalidReference)? as usize;
    let room = w.buf_length.min(arr_len.saturating_sub(w.offset));
    let mut buf: Vec<u8> = Vec::new();
    buf.try_reserve_exact(room)
        .map_err(|_| JvmError::StackOverflow)?;
    buf.resize(room, 0);
    Ok(buf)
}

/// Copy a received datagram into the packet's window at `offset`; the
/// buffer was sized by [`receive_buffer`], so every store lands.
pub(crate) fn store_payload(
    arrays: &mut ArrayHeap,
    w: &Window,
    data: &[u8],
) -> Result<(), JvmError> {
    for (i, &b) in data.iter().enumerate() {
        arrays
            .store(w.arr, w.offset + i, b as i8 as i32)
            .ok_or(JvmError::ArrayIndexOutOfBounds)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::array_heap::ATYPE_BYTE;

    fn byte_array(arrays: &mut ArrayHeap, bytes: &[u8]) -> u16 {
        let arr = arrays.alloc(ATYPE_BYTE, bytes.len() as u16).expect("array");
        for (i, &b) in bytes.iter().enumerate() {
            arrays.store(arr, i, b as i8 as i32).expect("store");
        }
        arr
    }

    /// A 300-byte datagram goes out whole, from the packet's offset; the
    /// 256-byte stack buffer that stood here sent it cut without a word.
    #[test]
    fn payload_is_the_whole_window_from_its_offset() {
        let mut arrays = ArrayHeap::new();
        let bytes: Vec<u8> = (0..310).map(|i| (i % 251) as u8).collect();
        let arr = byte_array(&mut arrays, &bytes);
        let w = Window {
            arr,
            offset: 10,
            length: 300,
            buf_length: 300,
        };
        assert_eq!(payload_of(&arrays, &w).unwrap(), &bytes[10..310]);
    }

    #[test]
    fn a_window_past_the_array_is_an_index_fault_not_a_short_send() {
        let mut arrays = ArrayHeap::new();
        let arr = byte_array(&mut arrays, &[1, 2, 3, 4]);
        let w = Window {
            arr,
            offset: 2,
            length: 3,
            buf_length: 3,
        };
        assert_eq!(
            payload_of(&arrays, &w).err(),
            Some(JvmError::ArrayIndexOutOfBounds)
        );
    }

    /// `receive` fills the window from the offset and leaves the rest of the
    /// array alone; the buffer is sized to the window, not to a constant.
    #[test]
    fn receive_buffer_is_the_window_and_the_payload_lands_at_the_offset() {
        let mut arrays = ArrayHeap::new();
        let arr = byte_array(&mut arrays, &[0u8; 1500]);
        let w = Window {
            arr,
            offset: 4,
            length: 1496,
            buf_length: 1496,
        };
        let buf = receive_buffer(&arrays, &w).unwrap();
        assert_eq!(buf.len(), 1496);
        let datagram: Vec<u8> = (0..300).map(|i| (255 - i % 256) as u8).collect();
        store_payload(&mut arrays, &w, &datagram).unwrap();
        assert_eq!(arrays.load(arr, 3), Some(0), "before the offset untouched");
        assert_eq!(arrays.load(arr, 4), Some(255u8 as i8 as i32));
        assert_eq!(arrays.load(arr, 4 + 299), Some(datagram[299] as i8 as i32));
        assert_eq!(
            arrays.load(arr, 4 + 300),
            Some(0),
            "past the datagram untouched"
        );
    }

    /// A window the packet claims but the array does not hold is cut to the
    /// array, so a shorter buffer is never written past its end.
    #[test]
    fn receive_buffer_is_cut_to_the_array() {
        let mut arrays = ArrayHeap::new();
        let arr = byte_array(&mut arrays, &[0u8; 64]);
        let w = Window {
            arr,
            offset: 60,
            length: 64,
            buf_length: 64,
        };
        assert_eq!(receive_buffer(&arrays, &w).unwrap().len(), 4);
    }
}
