// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;

use crate::{
    array_heap::{
        ArrayHeap, ATYPE_BYTE, ATYPE_CHAR, ATYPE_DOUBLE, ATYPE_FLOAT, ATYPE_INT, ATYPE_LONG,
        ATYPE_SHORT,
    },
    object_heap::{float_to_str_buf, int_to_decimal_buf, long_to_decimal_buf},
    sort::{
        f32_bits_from_key, f64_bits_from_key, i64_from_key, key_from_f32_bits, key_from_f64_bits,
        key_from_i64, sort_keys,
    },
    types::{JvmError, Value},
};

use super::NativeContext;

fn extract_array(args: &[Value]) -> Result<u16, JvmError> {
    match args.first().copied().unwrap_or(Value::Null) {
        Value::ArrayRef(i) => Ok(i),
        _ => Err(JvmError::InvalidReference),
    }
}

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "sort" => Some(dispatch_sort(ctx)),
        "fill" => Some(dispatch_fill(ctx)),
        "copyOf" => Some(dispatch_copy_of(ctx)),
        "toString" => Some(dispatch_to_string(ctx)),
        _ => None,
    }
}

// ── sort ─────────────────────────────────────────────────────────────────

fn dispatch_sort(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let arr = extract_array(ctx.args)?;
    let atype = ctx.arrays.atype(arr).ok_or(JvmError::InvalidReference)?;
    let len = ctx.arrays.length(arr).ok_or(JvmError::InvalidReference)? as usize;
    if len < 2 {
        return Ok(None);
    }
    match atype {
        ATYPE_INT => sort_i32(ctx.arrays, arr, len, |x| x),
        ATYPE_SHORT => sort_i32(ctx.arrays, arr, len, |x| x as i16 as i32),
        ATYPE_BYTE => sort_i32(ctx.arrays, arr, len, |x| x as i8 as i32),
        ATYPE_CHAR => sort_i32(ctx.arrays, arr, len, |x| x as u16 as i32),
        ATYPE_LONG => sort_i64(ctx.arrays, arr, len),
        ATYPE_FLOAT => sort_f32(ctx.arrays, arr, len),
        ATYPE_DOUBLE => sort_f64(ctx.arrays, arr, len),
        _ => return Err(JvmError::InvalidReference),
    }
    Ok(None)
}

/// In-place sort for i32-slot arrays. `widen` converts the raw stored i32
/// into a comparable i32 using the element type's signedness rules (byte[]
/// sign-extends, char[] zero-extends). It is a plain `fn` pointer, not a
/// generic: a generic parameter would monomorphise this loader four times
/// for no benefit.
fn sort_i32(arrays: &mut ArrayHeap, arr: u16, len: usize, widen: fn(i32) -> i32) {
    // Pull into a Vec, sort, write back. Cheaper than O(n log n) load/store
    // through the ArrayHeap accessors, and bounded — the array already exists
    // in heap so we know it fits.
    let mut buf: Vec<u64> = (0..len)
        .map(|i| key_from_i64(widen(arrays.load(arr, i).unwrap_or(0)) as i64))
        .collect();
    sort_keys(&mut buf);
    for (i, k) in buf.into_iter().enumerate() {
        let _ = arrays.store(arr, i, i64_from_key(k) as i32);
    }
}

fn sort_i64(arrays: &mut ArrayHeap, arr: u16, len: usize) {
    let mut buf: Vec<u64> = (0..len)
        .map(|i| key_from_i64(arrays.load64(arr, i).unwrap_or(0)))
        .collect();
    sort_keys(&mut buf);
    for (i, k) in buf.into_iter().enumerate() {
        let _ = arrays.store64(arr, i, i64_from_key(k));
    }
}

fn sort_f32(arrays: &mut ArrayHeap, arr: u16, len: usize) {
    // Float arrays use 1 i32 slot per element — bit-cast from raw i32.
    let mut buf: Vec<u64> = (0..len)
        .map(|i| key_from_f32_bits(arrays.load(arr, i).unwrap_or(0) as u32))
        .collect();
    sort_keys(&mut buf);
    for (i, k) in buf.into_iter().enumerate() {
        let _ = arrays.store(arr, i, f32_bits_from_key(k) as i32);
    }
}

fn sort_f64(arrays: &mut ArrayHeap, arr: u16, len: usize) {
    let mut buf: Vec<u64> = (0..len)
        .map(|i| key_from_f64_bits(arrays.load64(arr, i).unwrap_or(0) as u64))
        .collect();
    sort_keys(&mut buf);
    for (i, k) in buf.into_iter().enumerate() {
        let _ = arrays.store64(arr, i, f64_bits_from_key(k) as i64);
    }
}

// ── fill ─────────────────────────────────────────────────────────────────

fn dispatch_fill(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let arr = extract_array(ctx.args)?;
    let atype = ctx.arrays.atype(arr).ok_or(JvmError::InvalidReference)?;
    let len = ctx.arrays.length(arr).ok_or(JvmError::InvalidReference)? as usize;
    let val = ctx.args.get(1).copied().unwrap_or(Value::Null);
    match (atype, val) {
        (ATYPE_INT, Value::Int(v))
        | (ATYPE_SHORT, Value::Int(v))
        | (ATYPE_BYTE, Value::Int(v))
        | (ATYPE_CHAR, Value::Int(v)) => {
            for i in 0..len {
                let _ = ctx.arrays.store(arr, i, v);
            }
        }
        (ATYPE_LONG, Value::Long(v)) => {
            for i in 0..len {
                let _ = ctx.arrays.store64(arr, i, v);
            }
        }
        (ATYPE_FLOAT, Value::Float(v)) => {
            let bits = v.to_bits() as i32;
            for i in 0..len {
                let _ = ctx.arrays.store(arr, i, bits);
            }
        }
        (ATYPE_DOUBLE, Value::Double(v)) => {
            let bits = v.to_bits() as i64;
            for i in 0..len {
                let _ = ctx.arrays.store64(arr, i, bits);
            }
        }
        _ => return Err(JvmError::InvalidReference),
    }
    Ok(None)
}

// ── copyOf ───────────────────────────────────────────────────────────────

fn dispatch_copy_of(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let arr = extract_array(ctx.args)?;
    let new_len = match ctx.args.get(1).copied().unwrap_or(Value::Null) {
        Value::Int(n) if n >= 0 => n as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    let atype = ctx.arrays.atype(arr).ok_or(JvmError::InvalidReference)?;
    let old_len = ctx.arrays.length(arr).ok_or(JvmError::InvalidReference)? as usize;
    if new_len > u16::MAX as usize {
        return Err(JvmError::StackOverflow);
    }
    let new_arr = ctx
        .arrays
        .alloc(atype, new_len as u16)
        .ok_or(JvmError::StackOverflow)?;
    let copy_n = core::cmp::min(old_len, new_len);
    let wide = atype == ATYPE_LONG || atype == ATYPE_DOUBLE;
    for i in 0..copy_n {
        if wide {
            let v = ctx.arrays.load64(arr, i).unwrap_or(0);
            let _ = ctx.arrays.store64(new_arr, i, v);
        } else {
            let v = ctx.arrays.load(arr, i).unwrap_or(0);
            let _ = ctx.arrays.store(new_arr, i, v);
        }
    }
    Ok(Some(Value::ArrayRef(new_arr)))
}

// ── System.arraycopy ─────────────────────────────────────────────────────

/// `java/lang/System` builtin: only `arraycopy` lives here (array machinery);
/// `currentTimeMillis` stays with the platform handler, which dispatches
/// before the builtins.
pub(crate) fn dispatch_system(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "arraycopy" => Some(dispatch_arraycopy(ctx)),
        _ => None,
    }
}

/// `System.arraycopy(src, srcPos, dest, destPos, length)` with Java's
/// contract: bad ranges throw IndexOutOfBoundsException, mismatched element
/// types throw ArrayStoreException, and overlapping self-copies behave like
/// memmove (the overlap region is read before it is overwritten).
fn dispatch_arraycopy(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let src = match ctx.args.first().copied() {
        Some(Value::ArrayRef(i)) => i,
        _ => return Err(JvmError::InvalidReference),
    };
    let dest = match ctx.args.get(2).copied() {
        Some(Value::ArrayRef(i)) => i,
        _ => return Err(JvmError::InvalidReference),
    };
    let (src_pos, dest_pos, len) = match (
        ctx.args.get(1).copied(),
        ctx.args.get(3).copied(),
        ctx.args.get(4).copied(),
    ) {
        (Some(Value::Int(a)), Some(Value::Int(b)), Some(Value::Int(c))) => (a, b, c),
        _ => return Err(JvmError::InvalidReference),
    };

    let src_type = ctx.arrays.atype(src).ok_or(JvmError::InvalidReference)?;
    let dest_type = ctx.arrays.atype(dest).ok_or(JvmError::InvalidReference)?;
    if src_type != dest_type {
        return Err(throw_named(ctx, "java/lang/ArrayStoreException"));
    }
    let src_len = ctx.arrays.length(src).ok_or(JvmError::InvalidReference)? as i64;
    let dest_len = ctx.arrays.length(dest).ok_or(JvmError::InvalidReference)? as i64;
    if src_pos < 0
        || dest_pos < 0
        || len < 0
        || src_pos as i64 + len as i64 > src_len
        || dest_pos as i64 + len as i64 > dest_len
    {
        return Err(throw_named(ctx, "java/lang/IndexOutOfBoundsException"));
    }

    let (src_pos, dest_pos, len) = (src_pos as usize, dest_pos as usize, len as usize);
    let wide = src_type == ATYPE_LONG || src_type == ATYPE_DOUBLE;
    let backward = src == dest && dest_pos > src_pos;
    for k in 0..len {
        let i = if backward { len - 1 - k } else { k };
        if wide {
            let v = ctx.arrays.load64(src, src_pos + i).unwrap_or(0);
            let _ = ctx.arrays.store64(dest, dest_pos + i, v);
        } else {
            let v = ctx.arrays.load(src, src_pos + i).unwrap_or(0);
            let _ = ctx.arrays.store(dest, dest_pos + i, v);
        }
    }
    Ok(None)
}

fn throw_named(ctx: &mut NativeContext<'_>, class: &'static str) -> JvmError {
    match ctx.objects.alloc(class) {
        Some(idx) => JvmError::Exception(idx),
        None => JvmError::StackOverflow,
    }
}

// ── toString ─────────────────────────────────────────────────────────────

fn dispatch_to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    // Java's Arrays.toString(null) returns the literal string "null".
    let arg = ctx.args.first().copied().unwrap_or(Value::Null);
    if matches!(arg, Value::Null) {
        let idx = ctx.strings.intern(b"null").ok_or(JvmError::StackOverflow)?;
        return Ok(Some(Value::Reference(idx)));
    }
    let arr = match arg {
        Value::ArrayRef(i) => i,
        _ => return Err(JvmError::InvalidReference),
    };
    let atype = ctx.arrays.atype(arr).ok_or(JvmError::InvalidReference)?;
    let len = ctx.arrays.length(arr).ok_or(JvmError::InvalidReference)? as usize;

    // Build directly into a Vec<u8> — sized like a StringBuilder body.
    let mut out: Vec<u8> = Vec::with_capacity(len * 4 + 2);
    out.push(b'[');
    for i in 0..len {
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        match atype {
            ATYPE_INT => write_i32(&mut out, ctx.arrays.load(arr, i).unwrap_or(0)),
            ATYPE_SHORT => write_i32(&mut out, ctx.arrays.load(arr, i).unwrap_or(0) as i16 as i32),
            ATYPE_BYTE => write_i32(&mut out, ctx.arrays.load(arr, i).unwrap_or(0) as i8 as i32),
            ATYPE_CHAR => write_i32(&mut out, ctx.arrays.load(arr, i).unwrap_or(0) as u16 as i32),
            ATYPE_LONG => write_i64(&mut out, ctx.arrays.load64(arr, i).unwrap_or(0)),
            ATYPE_FLOAT => write_f32(
                &mut out,
                f32::from_bits(ctx.arrays.load(arr, i).unwrap_or(0) as u32),
            ),
            ATYPE_DOUBLE => write_f64(
                &mut out,
                f64::from_bits(ctx.arrays.load64(arr, i).unwrap_or(0) as u64),
            ),
            _ => return Err(JvmError::InvalidReference),
        }
    }
    out.push(b']');
    let idx = ctx
        .strings
        .intern_dyn_owned(out)
        .ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::Reference(idx)))
}

fn write_i32(out: &mut Vec<u8>, v: i32) {
    let mut tmp = [0u8; 12];
    out.extend_from_slice(int_to_decimal_buf(v, &mut tmp));
}

fn write_i64(out: &mut Vec<u8>, v: i64) {
    let mut tmp = [0u8; 21];
    out.extend_from_slice(long_to_decimal_buf(v, &mut tmp));
}

fn write_f32(out: &mut Vec<u8>, v: f32) {
    let mut tmp = [0u8; 32];
    out.extend_from_slice(float_to_str_buf(v, &mut tmp));
}

fn write_f64(out: &mut Vec<u8>, v: f64) {
    // Reuses the f32 formatter — same precision loss as `StringBuilder.append(double)`.
    let mut tmp = [0u8; 32];
    out.extend_from_slice(float_to_str_buf(v as f32, &mut tmp));
}
