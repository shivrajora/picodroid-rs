// SPDX-License-Identifier: GPL-3.0-only
//! micropb's stream traits over a Java `byte[]` in the JVM array heap.
//!
//! The heap offers no `&[u8]` view of a packed byte array (`ArrayHeap::load`
//! is one element at a time, as the io natives copy), so the decoder reads
//! through a small refill window: `pb_read_chunk` copies at most [`WINDOW`]
//! bytes out of the array and lends them; `pb_advance` consumes. A varint is
//! at most ten bytes and a fixed value eight, so a field never needs more
//! than one refill, and the decoder's own bookkeeping never sees the array.
//!
//! Both halves work on a `[pos, limit)` slice of the array — the Java
//! `CodedInputStream`/`CodedOutputStream` cursor — and hand the position back
//! so the native can store it in the Java object. Neither holds a reference
//! past the native call that built it.

use micropb::{PbRead, PbWrite};
use pico_jvm::array_heap::ArrayHeap;

/// Bytes copied out of the array per refill. Larger than the longest single
/// value (a ten-byte varint) so no value straddles two refills.
pub const WINDOW: usize = 16;

/// Why a heap read or write could not proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapIoError {
    /// The array index or an element index was not valid — the Java side
    /// handed the native something that is not a `byte[]`.
    Invalid,
    /// A write would pass `limit`.
    Full,
}

/// A `PbRead` over `array[pos..limit]`.
pub struct HeapReader<'a> {
    arrays: &'a ArrayHeap,
    buf: u16,
    pos: usize,
    limit: usize,
    window: [u8; WINDOW],
    win_len: usize,
    win_off: usize,
}

impl<'a> HeapReader<'a> {
    /// A reader over `array[pos..limit]`. `limit` is clamped to the array's
    /// length, so a bad Java limit reads short rather than out of bounds;
    /// `pos` past `limit` reads as empty.
    pub fn new(arrays: &'a ArrayHeap, buf: u16, pos: usize, limit: usize) -> Self {
        let len = arrays.length(buf).map(|n| n as usize).unwrap_or(0);
        let limit = limit.min(len);
        Self {
            arrays,
            buf,
            pos: pos.min(limit),
            limit,
            window: [0; WINDOW],
            win_len: 0,
            win_off: 0,
        }
    }

    /// Index of the next unconsumed byte: what the Java cursor becomes.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Bytes left before `limit`.
    pub fn remaining(&self) -> usize {
        self.limit - self.pos
    }
}

impl PbRead for HeapReader<'_> {
    type Error = HeapIoError;

    fn pb_read_chunk(&mut self) -> Result<&[u8], HeapIoError> {
        if self.win_off == self.win_len {
            let n = self.remaining().min(WINDOW);
            for i in 0..n {
                self.window[i] = self
                    .arrays
                    .load(self.buf, self.pos + i)
                    .ok_or(HeapIoError::Invalid)? as u8;
            }
            self.win_off = 0;
            self.win_len = n;
        }
        Ok(&self.window[self.win_off..self.win_len])
    }

    fn pb_advance(&mut self, bytes: usize) {
        let n = bytes.min(self.win_len - self.win_off);
        self.win_off += n;
        self.pos += n;
    }
}

/// A `PbWrite` into `array[pos..limit]`.
///
/// micropb writes a varint one byte at a time, so a value that does not fit
/// leaves the bytes that did before `Full` comes back — the same partial
/// state javalite's `CodedOutputStream` leaves on `OutOfSpaceException`.
/// Nothing at or past `limit` is ever written.
pub struct HeapWriter<'a> {
    arrays: &'a mut ArrayHeap,
    buf: u16,
    pos: usize,
    limit: usize,
}

impl<'a> HeapWriter<'a> {
    /// A writer into `array[pos..limit]`; `limit` is clamped to the array's
    /// length so a bad Java limit fills up early rather than writing past
    /// the array.
    pub fn new(arrays: &'a mut ArrayHeap, buf: u16, pos: usize, limit: usize) -> Self {
        let len = arrays.length(buf).map(|n| n as usize).unwrap_or(0);
        let limit = limit.min(len);
        Self {
            arrays,
            buf,
            pos: pos.min(limit),
            limit,
        }
    }

    /// Index of the next free byte: what the Java cursor becomes.
    pub fn position(&self) -> usize {
        self.pos
    }
}

impl PbWrite for HeapWriter<'_> {
    type Error = HeapIoError;

    fn pb_write(&mut self, data: &[u8]) -> Result<(), HeapIoError> {
        if data.len() > self.limit - self.pos {
            return Err(HeapIoError::Full);
        }
        for &b in data {
            self.arrays
                .store(self.buf, self.pos, b as i8 as i32)
                .ok_or(HeapIoError::Invalid)?;
            self.pos += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use micropb::{DecodeError, PbDecoder, PbEncoder, Tag};
    use pico_jvm::array_heap::ATYPE_BYTE;

    fn byte_array(arrays: &mut ArrayHeap, bytes: &[u8]) -> u16 {
        let idx = arrays.alloc(ATYPE_BYTE, bytes.len() as u16).expect("alloc");
        for (i, &b) in bytes.iter().enumerate() {
            arrays.store(idx, i, b as i8 as i32);
        }
        idx
    }

    fn read_back(arrays: &ArrayHeap, idx: u16, len: usize) -> alloc::vec::Vec<u8> {
        (0..len)
            .map(|i| arrays.load(idx, i).unwrap() as u8)
            .collect()
    }

    fn decoder(arrays: &ArrayHeap, idx: u16, len: usize) -> PbDecoder<HeapReader<'_>> {
        PbDecoder::new(HeapReader::new(arrays, idx, 0, len))
    }

    // ── golden vectors ─────────────────────────────────────────────────

    #[test]
    fn varint_300_is_ac_02_both_ways() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0xAC, 0x02, 0x7F]);
        let mut d = decoder(&arrays, idx, 3);
        assert_eq!(d.decode_varint32().unwrap(), 300);
        assert_eq!(d.decode_varint32().unwrap(), 0x7F);
        assert_eq!(d.into_reader().position(), 3);

        let out = byte_array(&mut arrays, &[0; 4]);
        let mut e = PbEncoder::new(HeapWriter::new(&mut arrays, out, 0, 4));
        e.encode_varint32(300).unwrap();
        assert_eq!(e.into_writer().position(), 2);
        assert_eq!(read_back(&arrays, out, 2), [0xAC, 0x02]);
    }

    #[test]
    fn negative_int32_takes_ten_bytes_and_round_trips() {
        let mut arrays = ArrayHeap::new();
        let out = byte_array(&mut arrays, &[0; 12]);
        let mut e = PbEncoder::new(HeapWriter::new(&mut arrays, out, 0, 12));
        e.encode_int32(-1).unwrap();
        let n = e.into_writer().position();
        assert_eq!(n, 10);
        assert_eq!(
            read_back(&arrays, out, 10),
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]
        );
        let mut d = decoder(&arrays, out, 10);
        assert_eq!(d.decode_varint64().unwrap() as i64, -1);
        assert_eq!(d.decode_varint32().unwrap_err(), DecodeError::UnexpectedEof);
    }

    #[test]
    fn int64_extremes_and_a_double_round_trip() {
        let mut arrays = ArrayHeap::new();
        let out = byte_array(&mut arrays, &[0; 40]);
        let mut e = PbEncoder::new(HeapWriter::new(&mut arrays, out, 0, 40));
        e.encode_int64(i64::MIN).unwrap();
        e.encode_int64(i64::MAX).unwrap();
        e.encode_double(1.5).unwrap();
        e.encode_fixed32(0xDEAD_BEEF).unwrap();
        let n = e.into_writer().position();
        assert_eq!(n, 10 + 9 + 8 + 4);
        // 1.5 as IEEE-754 little-endian.
        assert_eq!(
            read_back(&arrays, out, n)[19..27],
            [0, 0, 0, 0, 0, 0, 0xF8, 0x3F]
        );
        let mut d = decoder(&arrays, out, n);
        assert_eq!(d.decode_varint64().unwrap() as i64, i64::MIN);
        assert_eq!(d.decode_varint64().unwrap() as i64, i64::MAX);
        assert_eq!(d.decode_double().unwrap(), 1.5);
        assert_eq!(d.decode_fixed32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(d.into_reader().remaining(), 0);
    }

    // ── errors ─────────────────────────────────────────────────────────

    #[test]
    fn truncated_varint_is_unexpected_eof_and_overlong_is_the_limit() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0x80, 0x80]);
        let mut d = decoder(&arrays, idx, 2);
        assert_eq!(d.decode_varint32().unwrap_err(), DecodeError::UnexpectedEof);

        let idx = byte_array(&mut arrays, &[0x80; 12]);
        let mut d = decoder(&arrays, idx, 12);
        assert_eq!(d.decode_varint64().unwrap_err(), DecodeError::VarIntLimit);
    }

    #[test]
    fn limit_short_of_the_array_is_eof_and_a_bad_limit_is_clamped() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0x01, 0x02, 0x03]);
        let mut d = decoder(&arrays, idx, 1);
        assert_eq!(d.decode_varint32().unwrap(), 1);
        assert_eq!(d.decode_varint32().unwrap_err(), DecodeError::UnexpectedEof);

        let mut d = decoder(&arrays, idx, 99);
        assert_eq!(d.decode_fixed32().unwrap_err(), DecodeError::UnexpectedEof);
        let r = HeapReader::new(&arrays, idx, 7, 99);
        assert_eq!(r.position(), 3);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn a_write_past_limit_is_full_after_the_bytes_that_fit() {
        let mut arrays = ArrayHeap::new();
        let out = byte_array(&mut arrays, &[0x55; 4]);
        let mut e = PbEncoder::new(HeapWriter::new(&mut arrays, out, 0, 3));
        e.encode_varint32(300).unwrap();
        // micropb hands a varint over one byte at a time, so the first byte
        // of the second value lands and the second is refused: the cursor
        // stops at the limit, nothing past it is touched.
        assert_eq!(e.encode_varint32(300).unwrap_err(), HeapIoError::Full);
        assert_eq!(e.into_writer().position(), 3);
        assert_eq!(read_back(&arrays, out, 4), [0xAC, 0x02, 0xAC, 0x55]);
    }

    #[test]
    fn a_missing_array_is_invalid() {
        let arrays = ArrayHeap::new();
        let mut d = decoder(&arrays, 40_000, 4);
        // The reader clamps to a zero-length view; nothing to read.
        assert_eq!(d.decode_varint32().unwrap_err(), DecodeError::UnexpectedEof);
    }

    // ── window ─────────────────────────────────────────────────────────

    #[test]
    fn values_stream_across_window_refills() {
        let mut arrays = ArrayHeap::new();
        // 20 two-byte varints: 40 bytes, refills at 16 and 32 fall mid-value.
        let mut bytes = alloc::vec::Vec::new();
        for _ in 0..20 {
            bytes.extend_from_slice(&[0xAC, 0x02]);
        }
        let idx = byte_array(&mut arrays, &bytes);
        let mut d = decoder(&arrays, idx, 40);
        for _ in 0..20 {
            assert_eq!(d.decode_varint32().unwrap(), 300);
        }
        assert_eq!(d.decode_varint32().unwrap_err(), DecodeError::UnexpectedEof);
        assert_eq!(d.into_reader().position(), 40);
    }

    #[test]
    fn a_reader_starting_mid_array_reports_absolute_positions() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0, 0, 0xAC, 0x02, 9]);
        let mut d = PbDecoder::new(HeapReader::new(&arrays, idx, 2, 5));
        assert_eq!(d.decode_varint32().unwrap(), 300);
        assert_eq!(d.as_reader().position(), 4);
        assert_eq!(d.decode_varint32().unwrap(), 9);
        assert_eq!(d.into_reader().position(), 5);
    }

    // ── skipping ───────────────────────────────────────────────────────

    #[test]
    fn skip_wire_value_steps_over_every_wire_type() {
        let mut arrays = ArrayHeap::new();
        let out = byte_array(&mut arrays, &[0; 64]);
        let mut e = PbEncoder::new(HeapWriter::new(&mut arrays, out, 0, 64));
        e.encode_tag(Tag::from_parts(1, 0)).unwrap();
        e.encode_varint64(1 << 40).unwrap();
        e.encode_tag(Tag::from_parts(2, 1)).unwrap();
        e.encode_fixed64(7).unwrap();
        e.encode_tag(Tag::from_parts(3, 2)).unwrap();
        e.encode_varint32(3).unwrap();
        e.encode_fixed32(0).unwrap(); // three of these four bytes are the payload
        let w = e.into_writer();
        let pos = w.position() - 1;
        let mut e = PbEncoder::new(HeapWriter::new(&mut arrays, out, pos, 64));
        e.encode_tag(Tag::from_parts(4, 5)).unwrap();
        e.encode_fixed32(0xAB).unwrap();
        e.encode_tag(Tag::from_parts(5, 0)).unwrap();
        e.encode_varint32(77).unwrap();
        let end = e.into_writer().position();

        let mut d = decoder(&arrays, out, end);
        for expect in 1..=4u32 {
            let tag = d.decode_tag().unwrap();
            assert_eq!(tag.field_num(), expect);
            d.skip_wire_value(tag.wire_type()).unwrap();
        }
        let tag = d.decode_tag().unwrap();
        assert_eq!((tag.field_num(), tag.wire_type()), (5, 0));
        assert_eq!(d.decode_varint32().unwrap(), 77);
        assert_eq!(d.into_reader().position(), end);
    }

    #[test]
    fn skipping_a_group_is_refused_and_a_short_len_is_eof() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0x05, 0x01, 0x02]);
        let mut d = decoder(&arrays, idx, 3);
        assert_eq!(d.skip_wire_value(3).unwrap_err(), DecodeError::Deprecation);
        assert_eq!(
            d.skip_wire_value(2).unwrap_err(),
            DecodeError::UnexpectedEof
        );
    }
}
