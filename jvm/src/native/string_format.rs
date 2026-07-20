// SPDX-License-Identifier: GPL-3.0-only
//! `java.lang.String.format(String, Object[])` — Java-subset printf formatter.
//!
//! Supports conversions `%s %d %x %X %o %c %b %f %e %g %n %%` with flags
//! (`-`, `0`, `+`, ` `, `,`, `#`), width, and precision. Mismatched arguments
//! or bad specifiers throw `IllegalFormatException`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::array_heap::REF_TAG;
use crate::types::{JvmError, Value};

use super::NativeContext;

#[derive(Default, Clone, Copy)]
struct Spec {
    minus: bool,
    zero: bool,
    plus: bool,
    space: bool,
    comma: bool,
    hash: bool,
    width: usize,
    precision: Option<usize>,
    conv: u8,
}

/// Decode a raw i32 slot read from an Object[] into a typed Value.
/// Encoding: 0 = Null, REF_TAG set = String reference, else ObjectRef.
fn decode_slot(raw: i32) -> Value {
    let u = raw as u32;
    if u == 0 {
        Value::Null
    } else if u & REF_TAG != 0 {
        Value::Reference((u & !REF_TAG) as u16)
    } else {
        Value::ObjectRef(u as u16)
    }
}

/// Build and return an IllegalFormatException for the exception unwinding path.
fn fmt_err(ctx: &mut NativeContext<'_>) -> JvmError {
    match ctx.objects.alloc("java/util/IllegalFormatException") {
        Some(idx) => JvmError::Exception(idx),
        None => JvmError::StackOverflow,
    }
}

/// Unbox an ObjectRef if it is a boxed primitive wrapper; otherwise return
/// the original Value so the caller can decide how to stringify it.
fn unbox(ctx: &NativeContext<'_>, v: Value) -> Value {
    if let Value::ObjectRef(idx) = v {
        if let Some(
            "java/lang/Integer"
            | "java/lang/Boolean"
            | "java/lang/Long"
            | "java/lang/Float"
            | "java/lang/Double"
            | "java/lang/Character"
            | "java/lang/Short"
            | "java/lang/Byte",
        ) = ctx.objects.class_name(idx)
        {
            return ctx.objects.get_field(idx, 0).unwrap_or(Value::Null);
        }
    }
    v
}

/// Extract an integer-like value. Returns `(i64 value, u64 low-bit unsigned view)`
/// suitable for signed decimal and for unsigned hex/octal respectively.
fn as_int(ctx: &NativeContext<'_>, v: Value) -> Option<(i64, u64)> {
    match unbox(ctx, v) {
        Value::Int(n) => Some((n as i64, (n as u32) as u64)),
        Value::Long(n) => Some((n, n as u64)),
        _ => None,
    }
}

/// Extract a float-like value as f64.
fn as_float(ctx: &NativeContext<'_>, v: Value) -> Option<f64> {
    match unbox(ctx, v) {
        Value::Float(f) => Some(f as f64),
        Value::Double(d) => Some(d),
        Value::Int(n) => Some(n as f64),
        Value::Long(n) => Some(n as f64),
        _ => None,
    }
}

/// Turn any Value into bytes for `%s`, appending into the caller's cleared
/// scratch buffer — `format()` reuses one buffer across all args instead of
/// allocating a fresh Vec per conversion (device-heap churn in log-heavy
/// loops).
fn stringify(ctx: &NativeContext<'_>, v: Value, dst: &mut Vec<u8>) {
    dst.clear();
    match unbox(ctx, v) {
        Value::Null => dst.extend_from_slice(b"null"),
        Value::Reference(idx) => {
            dst.extend_from_slice(ctx.strings.resolve(idx).unwrap_or("null").as_bytes())
        }
        Value::Int(n) => {
            let mut tmp = [0u8; 12];
            dst.extend_from_slice(crate::object_heap::int_to_decimal_buf(n, &mut tmp));
        }
        Value::Long(n) => {
            let mut tmp = [0u8; 21];
            dst.extend_from_slice(crate::object_heap::long_to_decimal_buf(n, &mut tmp));
        }
        Value::Float(f) => {
            let mut tmp = [0u8; 32];
            dst.extend_from_slice(crate::object_heap::float_to_str_buf(f, &mut tmp));
        }
        Value::Double(d) => {
            let mut tmp = [0u8; 32];
            dst.extend_from_slice(crate::object_heap::float_to_str_buf(d as f32, &mut tmp));
        }
        Value::ObjectRef(idx) => {
            let name = ctx.objects.class_name(idx).unwrap_or("Object");
            dst.extend_from_slice(name.as_bytes());
            dst.push(b'@');
            let mut tmp = [0u8; 12];
            dst.extend_from_slice(crate::object_heap::int_to_decimal_buf(idx as i32, &mut tmp));
        }
        Value::ArrayRef(idx) => {
            dst.extend_from_slice(b"[@");
            let mut tmp = [0u8; 12];
            dst.extend_from_slice(crate::object_heap::int_to_decimal_buf(idx as i32, &mut tmp));
        }
    }
}

/// Render a signed decimal magnitude into the cleared scratch buffer,
/// applying the `,` grouping flag. Digits are built in a stack buffer
/// (u64 max = 20 digits) — no allocation at all.
fn decimal_digits(mag: u64, comma: bool, dst: &mut Vec<u8>) {
    dst.clear();
    let mut tmp = [0u8; 20];
    let mut i = tmp.len();
    let mut v = mag;
    if v == 0 {
        i -= 1;
        tmp[i] = b'0';
    }
    while v > 0 {
        i -= 1;
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let digits = &tmp[i..];
    let n = digits.len();
    for (k, &d) in digits.iter().enumerate() {
        if comma && k > 0 && (n - k) % 3 == 0 {
            dst.push(b',');
        }
        dst.push(d);
    }
}

fn hex_digits(mut u: u64, upper: bool, dst: &mut Vec<u8>) {
    dst.clear();
    let lut: &[u8] = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    let mut tmp = [0u8; 16];
    let mut i = tmp.len();
    if u == 0 {
        i -= 1;
        tmp[i] = b'0';
    }
    while u > 0 {
        i -= 1;
        tmp[i] = lut[(u & 0xf) as usize];
        u >>= 4;
    }
    dst.extend_from_slice(&tmp[i..]);
}

fn oct_digits(mut u: u64, dst: &mut Vec<u8>) {
    dst.clear();
    let mut tmp = [0u8; 22];
    let mut i = tmp.len();
    if u == 0 {
        i -= 1;
        tmp[i] = b'0';
    }
    while u > 0 {
        i -= 1;
        tmp[i] = b'0' + (u & 7) as u8;
        u >>= 3;
    }
    dst.extend_from_slice(&tmp[i..]);
}

/// Apply width/flags to a numeric body (sign already decided).
fn pad_numeric(sign: &[u8], body: &[u8], spec: &Spec, out: &mut Vec<u8>) {
    let total = sign.len() + body.len();
    if total >= spec.width {
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
        return;
    }
    let pad = spec.width - total;
    if spec.minus {
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
        for _ in 0..pad {
            out.push(b' ');
        }
    } else if spec.zero {
        // Sign first, then zero-fill, then digits.
        out.extend_from_slice(sign);
        for _ in 0..pad {
            out.push(b'0');
        }
        out.extend_from_slice(body);
    } else {
        for _ in 0..pad {
            out.push(b' ');
        }
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
    }
}

/// Apply width + precision to a string (truncate by precision, pad by width).
fn pad_string(bytes: &[u8], spec: &Spec, out: &mut Vec<u8>) {
    let slice = match spec.precision {
        Some(p) if p < bytes.len() => &bytes[..p],
        _ => bytes,
    };
    if slice.len() >= spec.width {
        out.extend_from_slice(slice);
        return;
    }
    let pad = spec.width - slice.len();
    if spec.minus {
        out.extend_from_slice(slice);
        for _ in 0..pad {
            out.push(b' ');
        }
    } else {
        for _ in 0..pad {
            out.push(b' ');
        }
        out.extend_from_slice(slice);
    }
}

/// Decide numeric sign prefix from the signed value and `+`/` ` flags.
fn numeric_sign(neg: bool, spec: &Spec) -> &'static [u8] {
    if neg {
        b"-"
    } else if spec.plus {
        b"+"
    } else if spec.space {
        b" "
    } else {
        b""
    }
}

/// Parse a format specifier starting at `fmt[i]` where `fmt[i-1] == b'%'`.
/// On success returns `(Spec, new_index)` pointing past the conversion char.
fn parse_spec(fmt: &[u8], mut i: usize) -> Option<(Spec, usize)> {
    let mut s = Spec::default();
    // Flags
    loop {
        match fmt.get(i).copied()? {
            b'-' => s.minus = true,
            b'0' => s.zero = true,
            b'+' => s.plus = true,
            b' ' => s.space = true,
            b',' => s.comma = true,
            b'#' => s.hash = true,
            _ => break,
        }
        i += 1;
    }
    // Width
    while let Some(&b) = fmt.get(i) {
        if b.is_ascii_digit() {
            s.width = s.width * 10 + (b - b'0') as usize;
            i += 1;
        } else {
            break;
        }
    }
    // Precision
    if fmt.get(i).copied() == Some(b'.') {
        i += 1;
        let mut p = 0usize;
        while let Some(&b) = fmt.get(i) {
            if b.is_ascii_digit() {
                p = p * 10 + (b - b'0') as usize;
                i += 1;
            } else {
                break;
            }
        }
        s.precision = Some(p);
    }
    // Conversion
    s.conv = *fmt.get(i)?;
    i += 1;
    Some((s, i))
}

pub(super) fn format(ctx: &mut NativeContext<'_>) -> Option<Result<Option<Value>, JvmError>> {
    let fmt_idx = match ctx.args.first() {
        Some(Value::Reference(idx)) => *idx,
        Some(Value::Null) => return Some(Err(JvmError::InvalidReference)),
        _ => return Some(Err(JvmError::InvalidReference)),
    };
    let arr_idx = match ctx.args.get(1) {
        Some(Value::ArrayRef(idx)) => *idx,
        // Missing varargs array → treat as zero-length
        _ => return Some(Err(JvmError::InvalidReference)),
    };

    let fmt_bytes: Vec<u8> = ctx
        .strings
        .resolve(fmt_idx)
        .unwrap_or("")
        .as_bytes()
        .to_vec();
    let arr_len = ctx.arrays.length(arr_idx).unwrap_or(0) as usize;

    // Pre-snapshot all arg slots so we don't re-borrow ctx.arrays mid-loop.
    let mut args: Vec<Value> = Vec::with_capacity(arr_len);
    for i in 0..arr_len {
        let raw = ctx.arrays.load(arr_idx, i).unwrap_or(0);
        args.push(decode_slot(raw));
    }

    let mut out: Vec<u8> = Vec::with_capacity(fmt_bytes.len());
    // One conversion buffer reused across all args (stringify/digit helpers
    // clear + refill it) — no per-arg Vec churn on the device heap.
    let mut scratch: Vec<u8> = Vec::new();
    let mut arg_pos = 0usize;
    let mut i = 0usize;

    while i < fmt_bytes.len() {
        let b = fmt_bytes[i];
        if b != b'%' {
            out.push(b);
            i += 1;
            continue;
        }
        let (spec, next) = match parse_spec(&fmt_bytes, i + 1) {
            Some(t) => t,
            None => return Some(Err(fmt_err(ctx))),
        };
        i = next;

        match spec.conv {
            b'%' => out.push(b'%'),
            b'n' => out.push(b'\n'),
            b's' | b'S' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                stringify(ctx, v, &mut scratch);
                if spec.conv == b'S' {
                    for c in scratch.iter_mut() {
                        c.make_ascii_uppercase();
                    }
                }
                pad_string(&scratch, &spec, &mut out);
            }
            b'b' | b'B' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let truthy = match unbox(ctx, v) {
                    Value::Null => false,
                    Value::Int(n) if matches!(v, Value::ObjectRef(_)) => n != 0,
                    // If the slot itself was a primitive int passed as Boolean-typed
                    // arg, we treat non-zero as true; but Java boxes primitives so
                    // this is the ObjectRef/Reference path in practice.
                    _ => true,
                };
                let s: &[u8] = if truthy { b"true" } else { b"false" };
                scratch.clear();
                scratch.extend_from_slice(s);
                if spec.conv == b'B' {
                    for c in scratch.iter_mut() {
                        c.make_ascii_uppercase();
                    }
                }
                pad_string(&scratch, &spec, &mut out);
            }
            b'c' | b'C' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let ch = match unbox(ctx, v) {
                    Value::Int(n) => n as u8,
                    _ => return Some(Err(fmt_err(ctx))),
                };
                scratch.clear();
                scratch.push(ch);
                pad_string(&scratch, &spec, &mut out);
            }
            b'd' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let (signed, _u) = match as_int(ctx, v) {
                    Some(t) => t,
                    None => return Some(Err(fmt_err(ctx))),
                };
                let neg = signed < 0;
                let mag = if neg {
                    (signed as i128).unsigned_abs() as u64
                } else {
                    signed as u64
                };
                decimal_digits(mag, spec.comma, &mut scratch);
                let sign = numeric_sign(neg, &spec);
                pad_numeric(sign, &scratch, &spec, &mut out);
            }
            b'x' | b'X' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let (_, u) = match as_int(ctx, v) {
                    Some(t) => t,
                    None => return Some(Err(fmt_err(ctx))),
                };
                hex_digits(u, spec.conv == b'X', &mut scratch);
                // `#` prefix is counted toward width, so zero-pad sits between
                // the prefix and the digits — treat it like a sign for padding.
                let prefix: &[u8] = if spec.hash {
                    if spec.conv == b'X' {
                        b"0X"
                    } else {
                        b"0x"
                    }
                } else {
                    b""
                };
                pad_numeric(prefix, &scratch, &spec, &mut out);
            }
            b'o' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let (_, u) = match as_int(ctx, v) {
                    Some(t) => t,
                    None => return Some(Err(fmt_err(ctx))),
                };
                oct_digits(u, &mut scratch);
                let prefix: &[u8] = if spec.hash && !scratch.starts_with(b"0") {
                    b"0"
                } else {
                    b""
                };
                pad_numeric(prefix, &scratch, &spec, &mut out);
            }
            b'f' | b'e' | b'E' | b'g' | b'G' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let f = match as_float(ctx, v) {
                    Some(f) => f,
                    None => return Some(Err(fmt_err(ctx))),
                };
                let prec = spec.precision.unwrap_or(6);
                let neg = f.is_sign_negative() && !f.is_nan();
                let mag = if neg { -f } else { f };
                let body_string: String = match spec.conv {
                    b'f' => format!("{:.*}", prec, mag),
                    b'e' | b'E' => {
                        // Rust's `{:e}` uses `e0` style without padded exponent.
                        // Java uses e.g. `1.234560e+02` — build it manually.
                        let s = format!("{:.*e}", prec, mag);
                        normalize_exp(&s, spec.conv == b'E')
                    }
                    b'g' | b'G' => {
                        // %g picks between %e and %f based on magnitude. Rough rule:
                        // use %e if exponent < -4 or >= precision.
                        let exp = if mag == 0.0 {
                            0
                        } else {
                            libm::floor(libm::log10(mag)) as i32
                        };
                        let eff_prec = if prec == 0 { 1 } else { prec };
                        if exp < -4 || exp >= eff_prec as i32 {
                            let s = format!("{:.*e}", eff_prec - 1, mag);
                            normalize_exp(&s, spec.conv == b'G')
                        } else {
                            let p = (eff_prec as i32 - 1 - exp).max(0) as usize;
                            format!("{:.*}", p, mag)
                        }
                    }
                    _ => unreachable!(),
                };
                let body = body_string.into_bytes();
                let sign = numeric_sign(neg, &spec);
                pad_numeric(sign, &body, &spec, &mut out);
            }
            _ => return Some(Err(fmt_err(ctx))),
        }
    }

    let r = ctx
        .strings
        .intern_dyn_owned(out)
        .ok_or(JvmError::StackOverflow);
    Some(r.map(|idx| Some(Value::Reference(idx))))
}

/// Convert Rust's `{:e}` output (e.g. `"1.23e2"`, `"1.23e-3"`) into Java/C
/// style with a signed, at-least-two-digit exponent (`"1.23e+02"`, `"1.23e-03"`).
fn normalize_exp(s: &str, upper: bool) -> String {
    let bytes = s.as_bytes();
    let e_pos = match bytes.iter().position(|&b| b == b'e' || b == b'E') {
        Some(p) => p,
        None => return s.to_string(),
    };
    let mantissa = &s[..e_pos];
    let exp_str = &s[e_pos + 1..];
    let (sign, digits) = if let Some(rest) = exp_str.strip_prefix('-') {
        ("-", rest)
    } else if let Some(rest) = exp_str.strip_prefix('+') {
        ("+", rest)
    } else {
        ("+", exp_str)
    };
    let mut padded = String::from(mantissa);
    padded.push(if upper { 'E' } else { 'e' });
    padded.push_str(sign);
    if digits.len() < 2 {
        padded.push('0');
    }
    padded.push_str(digits);
    padded
}
