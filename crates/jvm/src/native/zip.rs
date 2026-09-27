// SPDX-License-Identifier: GPL-3.0-only
//! `java/util/zip/CRC32` — the two `private static native` steps the JDK's
//! own class hands to zlib: `update(int crc, int b)` and `updateBytes(int
//! crc, byte[] b, int off, int len)`. The Java side (`sdk/java/java/util/
//! zip/CRC32.java`) keeps the running value, checks the bounds and carries
//! the public API, so the class dispatches like any other class-file class
//! and only the per-byte loop runs here.
//!
//! The value in and out is the *finalised* checksum, as zlib's `crc32()`
//! defines it: 0 for no bytes, and feeding more bytes to a finalised value
//! continues the same stream. The reflected IEEE 802.3 polynomial, one bit
//! at a time — no 1 KB table, which matters on the RP2040's flash budget,
//! and a preferences blob or a packet is a few hundred bytes.

use crate::types::{JvmError, Value};

use super::NativeContext;
use crate::names::m;

/// The running form of one more byte: `state` is the bitwise complement of
/// the finalised checksum.
#[inline]
fn step(state: u32, b: u8) -> u32 {
    let mut c = state ^ b as u32;
    for _ in 0..8 {
        let mask = (c & 1).wrapping_neg();
        c = (c >> 1) ^ (0xEDB8_8320 & mask);
    }
    c
}

/// zlib's `crc32(crc, buf)`: continue the finalised checksum `crc` (0 to
/// start) over `bytes` and finalise again.
pub fn crc32_update(crc: u32, bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!crc, |state, &b| step(state, b))
}

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        // The public `update(int)` / `update(byte[]…)` are bytecode; only
        // the `(int, int)` step is native.
        m::update if ctx.descriptor == "(II)I" => Some(update(ctx)),
        m::updateBytes => Some(update_bytes(ctx)),
        _ => None,
    }
}

fn int_arg(ctx: &NativeContext<'_>, i: usize) -> Result<i32, JvmError> {
    match ctx.args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `update(int crc, int b)`: one byte, the low eight bits of `b`.
fn update(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let crc = int_arg(ctx, 0)? as u32;
    let b = int_arg(ctx, 1)? as u8;
    Ok(Some(Value::Int(crc32_update(crc, &[b]) as i32)))
}

/// `updateBytes(int crc, byte[] b, int off, int len)`. The Java caller has
/// null-checked and bounds-checked; a range the heap cannot serve is the
/// InvalidReference every other array native reports.
fn update_bytes(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let crc = int_arg(ctx, 0)? as u32;
    let arr = match ctx.args.get(1) {
        Some(Value::ArrayRef(i)) => *i,
        _ => return Err(JvmError::InvalidReference),
    };
    let off = int_arg(ctx, 2)?;
    let len = int_arg(ctx, 3)?;
    if off < 0 || len < 0 {
        return Err(JvmError::InvalidReference);
    }
    let (off, len) = (off as usize, len as usize);
    let total = ctx.arrays.length(arr).ok_or(JvmError::InvalidReference)? as usize;
    match off.checked_add(len) {
        Some(end) if end <= total => {}
        _ => return Err(JvmError::InvalidReference),
    }
    let mut state = !crc;
    for i in off..off + len {
        let b = ctx.arrays.load(arr, i).ok_or(JvmError::InvalidReference)? as u8;
        state = step(state, b);
    }
    Ok(Some(Value::Int((!state) as i32)))
}
