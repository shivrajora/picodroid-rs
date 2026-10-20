// SPDX-License-Identifier: GPL-3.0-only
//! `CodedInputStream` / `CodedOutputStream`'s native half, without the JVM
//! glue: micropb's codec over a Java `byte[]` in the array heap ([`heap_io`]),
//! and the one-value operations the natives perform on a `[pos, limit)`
//! cursor ([`read_at`], [`write_at`]).
//!
//! Every operation is one value on one stream: decode or encode with a
//! `PbDecoder` / `PbEncoder` built over that slice, and hand the position
//! after it back so the caller can store it in the Java object. Nothing
//! here holds a JVM reference past the call, allocates, or touches the
//! array beyond `[pos, limit)`. The Java side owns everything else: zigzag,
//! limits, slicing strings and byte arrays, sizes.
//!
//! `picodroid-core`'s `native_handler/protobuf` is the glue: it reads the
//! stream object's slots, calls in here, stores the position and throws.
//! The split is what lets `crates/fuzz-targets` drive the codec on the host
//! (docs/designs/fuzzing-2026-10.md §8).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;

pub mod heap_io;

pub use heap_io::{HeapIoError, HeapReader, HeapWriter, WINDOW};
pub use micropb;
pub use micropb::DecodeError;

use micropb::{PbDecoder, PbEncoder};
use pico_jvm::array_heap::ArrayHeap;
use pico_jvm::types::Value;

/// One `CodedInputStream` native, as Java can call it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOp {
    Tag,
    Varint32,
    Varint64,
    Fixed32,
    Fixed64,
    Double,
    /// `nativeSkipField`: step over one value of the tag's wire type.
    /// Groups (3, 4) and the unassigned types (6, 7) are refused.
    Skip {
        wire_type: u8,
    },
}

/// One `CodedOutputStream` native. The value travels as raw bits (a
/// `double` as `to_bits`), so one signature serves every encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOp {
    Varint32,
    Varint64,
    Fixed32,
    Fixed64,
    Double,
}

pub type ReadResult<T> = Result<T, DecodeError<HeapIoError>>;

/// Decode one value from `array[pos..limit]`. `Ok` carries the value
/// (`None` for a skip) and the position after it; on `Err` the caller
/// leaves its cursor where it was, as the Java stream does.
pub fn read_at(
    arrays: &ArrayHeap,
    buf: u16,
    pos: usize,
    limit: usize,
    op: ReadOp,
) -> ReadResult<(Option<Value>, usize)> {
    let mut dec = PbDecoder::new(HeapReader::new(arrays, buf, pos, limit));
    let r = match op {
        ReadOp::Tag => dec
            .decode_tag()
            .map(|t| Some(Value::Int(t.varint() as i32))),
        ReadOp::Varint32 => dec.decode_varint32().map(|v| Some(Value::Int(v as i32))),
        ReadOp::Varint64 => dec.decode_varint64().map(|v| Some(Value::Long(v as i64))),
        ReadOp::Fixed32 => dec.decode_fixed32().map(|v| Some(Value::Int(v as i32))),
        ReadOp::Fixed64 => dec.decode_fixed64().map(|v| Some(Value::Long(v as i64))),
        ReadOp::Double => dec.decode_double().map(|v| Some(Value::Double(v))),
        ReadOp::Skip { wire_type } => dec.skip_wire_value(wire_type).map(|()| None),
    };
    let end = dec.into_reader().position();
    r.map(|v| (v, end))
}

/// Encode `v` with `op` into `array[pos..limit]`. The position after
/// whatever landed comes back with the verdict: `Err(Full)` after the bytes
/// that fit (javalite's `OutOfSpaceException` leaves the same partial
/// state), `Err(Invalid)` for an index that is not a `byte[]`.
pub fn write_at(
    arrays: &mut ArrayHeap,
    buf: u16,
    pos: usize,
    limit: usize,
    v: u64,
    op: WriteOp,
) -> (usize, Result<(), HeapIoError>) {
    let mut enc = PbEncoder::new(HeapWriter::new(arrays, buf, pos, limit));
    let r = match op {
        WriteOp::Varint32 => enc.encode_varint32(v as u32),
        WriteOp::Varint64 => enc.encode_varint64(v),
        WriteOp::Fixed32 => enc.encode_fixed32(v as u32),
        WriteOp::Fixed64 => enc.encode_fixed64(v),
        WriteOp::Double => enc.encode_double(f64::from_bits(v)),
    };
    (enc.into_writer().position(), r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::array_heap::ATYPE_BYTE;

    fn byte_array(arrays: &mut ArrayHeap, bytes: &[u8]) -> u16 {
        let idx = arrays.alloc(ATYPE_BYTE, bytes.len() as u16).expect("alloc");
        for (i, &b) in bytes.iter().enumerate() {
            arrays.store(idx, i, b as i8 as i32);
        }
        idx
    }

    #[test]
    fn read_at_moves_past_the_value_and_write_at_reads_back() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0x08, 0xAC, 0x02, 0x7F]);
        let (tag, p) = read_at(&arrays, idx, 0, 4, ReadOp::Tag).unwrap();
        assert_eq!((tag, p), (Some(Value::Int(8)), 1));
        let (v, p) = read_at(&arrays, idx, p, 4, ReadOp::Varint32).unwrap();
        assert_eq!((v, p), (Some(Value::Int(300)), 3));
        let (v, p) = read_at(&arrays, idx, p, 4, ReadOp::Skip { wire_type: 0 }).unwrap();
        assert_eq!((v, p), (None, 4));
        assert_eq!(
            read_at(&arrays, idx, p, 4, ReadOp::Fixed32).unwrap_err(),
            DecodeError::UnexpectedEof
        );

        let out = byte_array(&mut arrays, &[0x55; 10]);
        let (p, r) = write_at(&mut arrays, out, 0, 10, 300, WriteOp::Varint32);
        assert_eq!((p, r), (2, Ok(())));
        let (p, r) = write_at(&mut arrays, out, p, 10, 1.5f64.to_bits(), WriteOp::Double);
        assert_eq!((p, r), (10, Ok(())));
        let (p, r) = write_at(&mut arrays, out, p, 10, 1, WriteOp::Fixed32);
        assert_eq!((p, r), (10, Err(HeapIoError::Full)));
        let (v, _) = read_at(&arrays, out, 2, 10, ReadOp::Double).unwrap();
        assert_eq!(v, Some(Value::Double(1.5)));
    }

    /// What `decode_varint32` does with a varint wider than 32 bits, pinned
    /// so the fuzz target's reference decoder encodes micropb's rule rather
    /// than a guess: the low 32 bits, as javalite's `readRawVarint32` keeps
    /// them, and `VarIntLimit` only past ten bytes.
    #[test]
    fn a_varint_wider_than_32_bits_keeps_its_low_bits() {
        let mut arrays = ArrayHeap::new();
        // 2^32 + 300 as a varint: AC 82 80 80 10.
        let idx = byte_array(&mut arrays, &[0xAC, 0x82, 0x80, 0x80, 0x10]);
        let (v, p) = read_at(&arrays, idx, 0, 5, ReadOp::Varint32).unwrap();
        assert_eq!((v, p), (Some(Value::Int(300)), 5));
        // Ten 0x80 bytes and a terminator: eleven bytes, over the limit.
        let idx = byte_array(&mut arrays, &[0x80; 11]);
        assert_eq!(
            read_at(&arrays, idx, 0, 11, ReadOp::Varint64).unwrap_err(),
            DecodeError::VarIntLimit
        );
    }
}
