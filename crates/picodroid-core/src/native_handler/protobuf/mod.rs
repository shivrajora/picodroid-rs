// SPDX-License-Identifier: GPL-3.0-only
//! `CodedInputStream` / `CodedOutputStream` natives: the byte-at-a-time half
//! of `picodroid.protobuf`, over micropb's codec and the array-heap adapter
//! in `heap_io`.
//!
//! Every arm is one value on one stream: it reads the stream's `mBuf`,
//! `mPos` and `mLimit` slots, decodes or encodes with a `PbDecoder` /
//! `PbEncoder` built over that slice, and stores the new position back.
//! Nothing here holds a JVM reference past the call, allocates, or touches
//! the array beyond `[mPos, mLimit)`. The Java side owns everything else:
//! zigzag, limits, slicing strings and byte arrays, sizes.
//!
//! Errors: a read that runs off the limit or meets a malformed varint throws
//! `InvalidProtocolBufferException` (the class declares no instance fields,
//! which is what lets `throw_exception` raise it without a constructor); a
//! write that does not fit answers `-1` and Java throws
//! `OutOfSpaceException`.

mod heap_io;

use micropb::{DecodeError, PbDecoder, PbEncoder};
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

use self::heap_io::{HeapIoError, HeapReader, HeapWriter};
use super::throw_exception;
use crate::shrink_names::{c, m};

pub mod fields;

const ST_OK: i32 = 0;
const ST_FULL: i32 = -1;

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if class_name == c::picodroid_protobuf_CodedInputStream {
        let r = match method_name {
            m::nativeReadTag => read(ctx, |d| {
                d.decode_tag().map(|t| Value::Int(t.varint() as i32))
            }),
            m::nativeReadVarint32 => {
                read(ctx, |d| d.decode_varint32().map(|v| Value::Int(v as i32)))
            }
            m::nativeReadVarint64 => {
                read(ctx, |d| d.decode_varint64().map(|v| Value::Long(v as i64)))
            }
            m::nativeReadFixed32 => read(ctx, |d| d.decode_fixed32().map(|v| Value::Int(v as i32))),
            m::nativeReadFixed64 => {
                read(ctx, |d| d.decode_fixed64().map(|v| Value::Long(v as i64)))
            }
            m::nativeReadDouble => read(ctx, |d| d.decode_double().map(Value::Double)),
            m::nativeSkipField => skip_field(ctx),
            _ => return None,
        };
        return Some(r);
    }
    if class_name == c::picodroid_protobuf_CodedOutputStream {
        let r = match method_name {
            m::nativeWriteVarint32 => match as_int(ctx.args.get(1)) {
                Ok(v) => write(ctx, |e| e.encode_varint32(v as u32)),
                Err(e) => Err(e),
            },
            m::nativeWriteVarint64 => match as_long(ctx.args.get(1)) {
                Ok(v) => write(ctx, |e| e.encode_varint64(v as u64)),
                Err(e) => Err(e),
            },
            m::nativeWriteFixed32 => match as_int(ctx.args.get(1)) {
                Ok(v) => write(ctx, |e| e.encode_fixed32(v as u32)),
                Err(e) => Err(e),
            },
            m::nativeWriteFixed64 => match as_long(ctx.args.get(1)) {
                Ok(v) => write(ctx, |e| e.encode_fixed64(v as u64)),
                Err(e) => Err(e),
            },
            m::nativeWriteDouble => match as_double(ctx.args.get(1)) {
                Ok(v) => write(ctx, |e| e.encode_double(v)),
                Err(e) => Err(e),
            },
            _ => return None,
        };
        return Some(r);
    }
    None
}

/// The `(mBuf, mPos, mLimit)` slots of one stream class.
#[derive(Clone, Copy)]
struct Slots {
    buf: usize,
    pos: usize,
    limit: usize,
}

const INPUT: Slots = Slots {
    buf: fields::coded_input_stream::BUF,
    pos: fields::coded_input_stream::POS,
    limit: fields::coded_input_stream::LIMIT,
};

const OUTPUT: Slots = Slots {
    buf: fields::coded_output_stream::BUF,
    pos: fields::coded_output_stream::POS,
    limit: fields::coded_output_stream::LIMIT,
};

/// The stream's `(self, mBuf, mPos, mLimit)`.
fn cursor(ctx: &NativeContext<'_>, slots: Slots) -> Result<(u16, u16, usize, usize), JvmError> {
    let this = as_obj(ctx.args.first())?;
    let buf = match ctx.objects.get_field(this, slots.buf) {
        Some(Value::ArrayRef(i)) => i,
        _ => return Err(JvmError::InvalidReference),
    };
    let pos = match ctx.objects.get_field(this, slots.pos) {
        Some(Value::Int(p)) => p.max(0) as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    let limit = match ctx.objects.get_field(this, slots.limit) {
        Some(Value::Int(l)) => l.max(0) as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    Ok((this, buf, pos, limit))
}

fn store_pos(
    ctx: &mut NativeContext<'_>,
    this: u16,
    slots: Slots,
    pos: usize,
) -> Result<(), JvmError> {
    ctx.objects
        .set_field(this, slots.pos, Value::Int(pos as i32))
        .ok_or(JvmError::InvalidReference)
}

type ReadResult<T> = Result<T, DecodeError<HeapIoError>>;

/// Decode one value at the cursor with `f`; on success the cursor moves past
/// it, on failure it stays and `InvalidProtocolBufferException` is thrown.
fn read(
    ctx: &mut NativeContext<'_>,
    f: impl FnOnce(&mut PbDecoder<HeapReader<'_>>) -> ReadResult<Value>,
) -> Result<Option<Value>, JvmError> {
    let (this, buf, pos, limit) = cursor(ctx, INPUT)?;
    let mut dec = PbDecoder::new(HeapReader::new(ctx.arrays, buf, pos, limit));
    let r = f(&mut dec);
    let end = dec.into_reader().position();
    match r {
        Ok(v) => {
            store_pos(ctx, this, INPUT, end)?;
            Ok(Some(v))
        }
        Err(e) => Err(decode_error(ctx, e)),
    }
}

/// `nativeSkipField(self, tag)`: step over one value of the tag's wire
/// type. Groups are refused, as everywhere else in this package.
fn skip_field(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let tag = as_int(ctx.args.get(1))?;
    let wire_type = (tag & 0x7) as u8;
    let (this, buf, pos, limit) = cursor(ctx, INPUT)?;
    let mut dec = PbDecoder::new(HeapReader::new(ctx.arrays, buf, pos, limit));
    let r = dec.skip_wire_value(wire_type);
    let end = dec.into_reader().position();
    match r {
        Ok(()) => {
            store_pos(ctx, this, INPUT, end)?;
            Ok(None)
        }
        Err(e) => Err(decode_error(ctx, e)),
    }
}

/// Encode one value at the cursor with `f`; the cursor moves past whatever
/// was written, and the answer says whether all of it fit.
fn write(
    ctx: &mut NativeContext<'_>,
    f: impl FnOnce(&mut PbEncoder<HeapWriter<'_>>) -> Result<(), HeapIoError>,
) -> Result<Option<Value>, JvmError> {
    let (this, buf, pos, limit) = cursor(ctx, OUTPUT)?;
    let mut enc = PbEncoder::new(HeapWriter::new(ctx.arrays, buf, pos, limit));
    let r = f(&mut enc);
    let end = enc.into_writer().position();
    store_pos(ctx, this, OUTPUT, end)?;
    match r {
        Ok(()) => Ok(Some(Value::Int(ST_OK))),
        Err(HeapIoError::Full) => Ok(Some(Value::Int(ST_FULL))),
        Err(HeapIoError::Invalid) => Err(JvmError::InvalidReference),
    }
}

fn decode_error(ctx: &mut NativeContext<'_>, e: DecodeError<HeapIoError>) -> JvmError {
    let msg = match e {
        DecodeError::UnexpectedEof => "truncated message",
        DecodeError::VarIntLimit => "malformed varint",
        DecodeError::ZeroField => "invalid tag (zero)",
        DecodeError::Deprecation => "groups are not supported",
        DecodeError::UnknownWireType => "invalid wire type",
        DecodeError::Reader(HeapIoError::Invalid) => return JvmError::InvalidReference,
        _ => "malformed message",
    };
    throw_exception(
        ctx,
        c::picodroid_protobuf_InvalidProtocolBufferException,
        msg,
    )
}

fn as_obj(v: Option<&Value>) -> Result<u16, JvmError> {
    match v {
        Some(Value::ObjectRef(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_int(v: Option<&Value>) -> Result<i32, JvmError> {
    match v {
        Some(Value::Int(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_long(v: Option<&Value>) -> Result<i64, JvmError> {
    match v {
        Some(Value::Long(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_double(v: Option<&Value>) -> Result<f64, JvmError> {
    match v {
        Some(Value::Double(d)) => Ok(*d),
        _ => Err(JvmError::InvalidReference),
    }
}
