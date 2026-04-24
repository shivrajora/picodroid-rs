use alloc::vec::Vec;

use crate::{
    array_heap::{
        ArrayHeap, ATYPE_BYTE, ATYPE_CHAR, ATYPE_DOUBLE, ATYPE_FLOAT, ATYPE_INT, ATYPE_LONG,
        ATYPE_SHORT,
    },
    object_heap::{float_to_str_buf, int_to_decimal_buf, long_to_decimal_buf},
    types::{JvmError, Value},
};

use super::NativeContext;

/// Below this size, sort with insertion sort to avoid quicksort overhead.
const INSERTION_THRESHOLD: usize = 16;

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

/// Generic in-place sort for i32-slot arrays. `widen` converts the raw stored
/// i32 into a comparable i32 using the array's element type's signedness rules
/// (e.g. byte[] sign-extends, char[] zero-extends).
fn sort_i32<F: Fn(i32) -> i32 + Copy>(arrays: &mut ArrayHeap, arr: u16, len: usize, widen: F) {
    // Pull into a Vec, sort, write back. Cheaper than O(n log n) load/store
    // through the ArrayHeap accessors, and bounded — the array already exists
    // in heap so we know it fits.
    let mut buf: Vec<i32> = (0..len)
        .map(|i| widen(arrays.load(arr, i).unwrap_or(0)))
        .collect();
    if len < INSERTION_THRESHOLD {
        insertion_sort(&mut buf, |a, b| a.cmp(b));
    } else {
        buf.sort();
    }
    for (i, v) in buf.into_iter().enumerate() {
        let _ = arrays.store(arr, i, v);
    }
}

fn sort_i64(arrays: &mut ArrayHeap, arr: u16, len: usize) {
    let mut buf: Vec<i64> = (0..len)
        .map(|i| arrays.load64(arr, i).unwrap_or(0))
        .collect();
    if len < INSERTION_THRESHOLD {
        insertion_sort(&mut buf, |a, b| a.cmp(b));
    } else {
        buf.sort();
    }
    for (i, v) in buf.into_iter().enumerate() {
        let _ = arrays.store64(arr, i, v);
    }
}

fn sort_f32(arrays: &mut ArrayHeap, arr: u16, len: usize) {
    // Float arrays use 1 i32 slot per element — bit-cast from raw i32.
    let mut buf: Vec<f32> = (0..len)
        .map(|i| f32::from_bits(arrays.load(arr, i).unwrap_or(0) as u32))
        .collect();
    if len < INSERTION_THRESHOLD {
        insertion_sort(&mut buf, f32::total_cmp);
    } else {
        buf.sort_by(f32::total_cmp);
    }
    for (i, v) in buf.into_iter().enumerate() {
        let _ = arrays.store(arr, i, v.to_bits() as i32);
    }
}

fn sort_f64(arrays: &mut ArrayHeap, arr: u16, len: usize) {
    let mut buf: Vec<f64> = (0..len)
        .map(|i| f64::from_bits(arrays.load64(arr, i).unwrap_or(0) as u64))
        .collect();
    if len < INSERTION_THRESHOLD {
        insertion_sort(&mut buf, f64::total_cmp);
    } else {
        buf.sort_by(f64::total_cmp);
    }
    for (i, v) in buf.into_iter().enumerate() {
        let _ = arrays.store64(arr, i, v.to_bits() as i64);
    }
}

fn insertion_sort<T: Copy, F: Fn(&T, &T) -> core::cmp::Ordering>(buf: &mut [T], cmp: F) {
    for i in 1..buf.len() {
        let key = buf[i];
        let mut j = i;
        while j > 0 && cmp(&buf[j - 1], &key) == core::cmp::Ordering::Greater {
            buf[j] = buf[j - 1];
            j -= 1;
        }
        buf[j] = key;
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
        .intern_dyn(&out)
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
