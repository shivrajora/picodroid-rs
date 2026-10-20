// SPDX-License-Identifier: GPL-3.0-only
//! `CodedInputStream` / `CodedOutputStream` natives: the JVM glue over the
//! `pd-protobuf` crate, which holds micropb's codec over the array heap and
//! the one-value cursor operations (`read_at`, `write_at`).
//!
//! Every arm is one value on one stream: it reads the stream's `mBuf`,
//! `mPos` and `mLimit` slots, asks `pd_protobuf` to decode or encode at
//! that cursor, and stores the new position back. Nothing here holds a JVM
//! reference past the call, allocates, or touches the array beyond
//! `[mPos, mLimit)`. The Java side owns everything else: zigzag, limits,
//! slicing strings and byte arrays, sizes.
//!
//! Errors: a read that runs off the limit or meets a malformed varint throws
//! `InvalidProtocolBufferException` (the class declares no instance fields,
//! which is what lets `throw_exception` raise it without a constructor); a
//! write that does not fit answers `-1` and Java throws
//! `OutOfSpaceException`.
use crate::shrink_names::{c, ch, mh};
use pico_jvm::NativeKey;

use pd_protobuf::{read_at, write_at, DecodeError, HeapIoError, ReadOp, WriteOp};
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

use super::throw_exception;

pub mod fields;

const ST_OK: i32 = 0;
const ST_FULL: i32 = -1;

pub fn dispatch(
    key: NativeKey,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if key.class == ch::picodroid_protobuf_CodedInputStream {
        let r = match key.method {
            mh::nativeReadTag => read(ctx, ReadOp::Tag),
            mh::nativeReadVarint32 => read(ctx, ReadOp::Varint32),
            mh::nativeReadVarint64 => read(ctx, ReadOp::Varint64),
            mh::nativeReadFixed32 => read(ctx, ReadOp::Fixed32),
            mh::nativeReadFixed64 => read(ctx, ReadOp::Fixed64),
            mh::nativeReadDouble => read(ctx, ReadOp::Double),
            mh::nativeSkipField => match as_int(ctx.args.get(1)) {
                Ok(tag) => read(
                    ctx,
                    ReadOp::Skip {
                        wire_type: (tag & 0x7) as u8,
                    },
                ),
                Err(e) => Err(e),
            },
            _ => return None,
        };
        return Some(r);
    }
    if key.class == ch::picodroid_protobuf_CodedOutputStream {
        let r = match key.method {
            mh::nativeWriteVarint32 => match as_int(ctx.args.get(1)) {
                Ok(v) => write(ctx, v as u32 as u64, WriteOp::Varint32),
                Err(e) => Err(e),
            },
            mh::nativeWriteVarint64 => match as_long(ctx.args.get(1)) {
                Ok(v) => write(ctx, v as u64, WriteOp::Varint64),
                Err(e) => Err(e),
            },
            mh::nativeWriteFixed32 => match as_int(ctx.args.get(1)) {
                Ok(v) => write(ctx, v as u32 as u64, WriteOp::Fixed32),
                Err(e) => Err(e),
            },
            mh::nativeWriteFixed64 => match as_long(ctx.args.get(1)) {
                Ok(v) => write(ctx, v as u64, WriteOp::Fixed64),
                Err(e) => Err(e),
            },
            mh::nativeWriteDouble => match as_double(ctx.args.get(1)) {
                Ok(v) => write(ctx, v.to_bits(), WriteOp::Double),
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

/// Decode one value at the cursor; on success the cursor moves past it, on
/// failure it stays and `InvalidProtocolBufferException` is thrown. One
/// copy of the cursor and error handling serves every decoder: the op is
/// data, matched inside `read_at`.
fn read(ctx: &mut NativeContext<'_>, op: ReadOp) -> Result<Option<Value>, JvmError> {
    let (this, buf, pos, limit) = cursor(ctx, INPUT)?;
    match read_at(ctx.arrays, buf, pos, limit, op) {
        Ok((v, end)) => {
            store_pos(ctx, this, INPUT, end)?;
            Ok(v)
        }
        Err(e) => Err(decode_error(ctx, e)),
    }
}

/// Encode `v` at the cursor; the cursor moves past whatever was written,
/// and the answer says whether all of it fit. `v` carries the value as raw
/// bits (a `double` as `to_bits`), as for [`read`]: one copy of the cursor
/// handling for every encoder.
fn write(ctx: &mut NativeContext<'_>, v: u64, op: WriteOp) -> Result<Option<Value>, JvmError> {
    let (this, buf, pos, limit) = cursor(ctx, OUTPUT)?;
    let (end, r) = write_at(ctx.arrays, buf, pos, limit, v, op);
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
