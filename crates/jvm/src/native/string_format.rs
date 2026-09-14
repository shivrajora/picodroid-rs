// SPDX-License-Identifier: GPL-3.0-only
//! `java.lang.String.format(String, Object[])` — Java-subset printf formatter.
//!
//! Supports conversions `%s %d %x %X %o %c %b %f %e %g %n %%` with flags
//! (`-`, `0`, `+`, ` `, `,`, `#`), width, and precision. Mismatched arguments
//! or bad specifiers throw `IllegalFormatException`.

use alloc::vec::Vec;

use crate::array_heap::decode_ref;
use crate::types::{JvmError, Value};

use super::NativeContext;
use crate::names::c;

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

/// Make room for `n` more bytes in a formatter buffer without aborting.
/// Every buffer in this module grows through here: a heap that cannot
/// spare the bytes is the allocation-failure signal, which the interpreter
/// turns into a collection and a retry, then OutOfMemoryError — where an
/// infallible `Vec::push` reset the board (QA 2026-09-13: qa_coll's
/// formatting section on testbench_rp2040, "memory allocation of 96 bytes
/// failed").
fn room(v: &mut Vec<u8>, n: usize) -> Result<(), JvmError> {
    crate::object_heap::reserve_fallible(v, n).map_err(|_| JvmError::StackOverflow)
}

/// A fixed-capacity `core::fmt::Write` target for `{:e}` of an f64 (at
/// most 24 bytes): the float conversions allocate nothing.
struct StackBuf {
    buf: [u8; 32],
    len: usize,
}

impl core::fmt::Write for StackBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        if self.len + b.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
        Ok(())
    }
}

/// Build and return an IllegalFormatException for the exception unwinding path.
fn fmt_err(ctx: &mut NativeContext<'_>) -> JvmError {
    match ctx.objects.alloc(c::java_util_IllegalFormatException) {
        Some(idx) => JvmError::Exception(idx),
        None => JvmError::StackOverflow,
    }
}

/// Unbox an ObjectRef if it is a boxed primitive wrapper; otherwise return
/// the original Value so the caller can decide how to stringify it.
fn unbox(ctx: &NativeContext<'_>, v: Value) -> Value {
    if let Value::ObjectRef(idx) = v {
        if let Some(
            c::java_lang_Integer
            | c::java_lang_Boolean
            | c::java_lang_Long
            | c::java_lang_Float
            | c::java_lang_Double
            | c::java_lang_Character
            | c::java_lang_Short
            | c::java_lang_Byte,
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

/// The unsigned view of an integer argument at its own width, for `%x` and
/// `%o`: a `Byte` box is 8 bits and a `Short` 16 (`%x` of `(byte) -1` is
/// `ff`, as in Java), an int 32, a long 64.
fn as_unsigned(ctx: &NativeContext<'_>, v: Value) -> Option<u64> {
    let (_, u) = as_int(ctx, v)?;
    Some(match v {
        Value::ObjectRef(idx) => match ctx.objects.class_name(idx) {
            Some(c::java_lang_Byte) => u & 0xff,
            Some(c::java_lang_Short) => u & 0xffff,
            _ => u,
        },
        _ => u,
    })
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
fn stringify(ctx: &NativeContext<'_>, v: Value, dst: &mut Vec<u8>) -> Result<(), JvmError> {
    dst.clear();
    // Every non-string form below fits in 64 bytes (a class name is short
    // and `long`/`double` renderings are bounded); a string reserves its own.
    room(dst, 64)?;
    // A `Boolean` prints `true`/`false` and a `Character` its character, as
    // their `toString` does (QA 2026-09-13: `%s` printed the raw int).
    if let Value::ObjectRef(idx) = v {
        match ctx.objects.class_name(idx) {
            Some(c::java_lang_Boolean) => {
                let set = matches!(ctx.objects.get_field(idx, 0), Some(Value::Int(n)) if n != 0);
                dst.extend_from_slice(if set { b"true" } else { b"false" });
                return Ok(());
            }
            Some(c::java_lang_Character) => {
                if let Some(Value::Int(n)) = ctx.objects.get_field(idx, 0) {
                    dst.push(n as u8);
                    return Ok(());
                }
            }
            _ => {}
        }
    }
    let unboxed = unbox(ctx, v);
    match unboxed {
        Value::Null => dst.extend_from_slice(b"null"),
        Value::Reference(idx) => {
            let s = ctx.strings.resolve(idx).unwrap_or("null").as_bytes();
            room(dst, s.len())?;
            dst.extend_from_slice(s)
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
            dst.extend_from_slice(crate::object_heap::double_to_str_buf(d, &mut tmp));
        }
        Value::ObjectRef(idx) => {
            // Identity shape, matching Object.toString (dotted name @ 4-hex
            // index). Objects with a toString() override never reach here
            // when the interpreter is driving: op_invoke pre-stringifies
            // format's Object[] elements (bugbash S4) — doing the upcall
            // from this native would monomorphise a second Executor for
            // BuiltinHandler, which overflowed RP2040's flash by 11.7 KB.
            let name = ctx.objects.class_name(idx).unwrap_or("Object");
            room(dst, name.len() + 8)?;
            for b in name.bytes() {
                dst.push(if b == b'/' { b'.' } else { b });
            }
            dst.push(b'@');
            for shift in [12u32, 8, 4, 0] {
                dst.push(b"0123456789abcdef"[((idx >> shift) & 0xF) as usize]);
            }
        }
        Value::ArrayRef(idx) => {
            dst.extend_from_slice(b"[@");
            let mut tmp = [0u8; 12];
            dst.extend_from_slice(crate::object_heap::int_to_decimal_buf(idx as i32, &mut tmp));
        }
    }
    Ok(())
}

/// Render a signed decimal magnitude into the cleared scratch buffer,
/// applying the `,` grouping flag. Digits are built in a stack buffer
/// (u64 max = 20 digits) — no allocation at all.
fn decimal_digits(mag: u64, comma: bool, dst: &mut Vec<u8>) -> Result<(), JvmError> {
    dst.clear();
    room(dst, 27)?;
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
    Ok(())
}

fn hex_digits(mut u: u64, upper: bool, dst: &mut Vec<u8>) -> Result<(), JvmError> {
    dst.clear();
    room(dst, 16)?;
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
    Ok(())
}

fn oct_digits(mut u: u64, dst: &mut Vec<u8>) -> Result<(), JvmError> {
    dst.clear();
    room(dst, 22)?;
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
    Ok(())
}

/// Apply width/flags to a numeric body (sign already decided).
fn pad_numeric(sign: &[u8], body: &[u8], spec: &Spec, out: &mut Vec<u8>) -> Result<(), JvmError> {
    let total = sign.len() + body.len();
    room(out, total.max(spec.width))?;
    if total >= spec.width {
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
        return Ok(());
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
    Ok(())
}

/// Apply width + precision to a string (truncate by precision, pad by width).
fn pad_string(bytes: &[u8], spec: &Spec, out: &mut Vec<u8>) -> Result<(), JvmError> {
    let slice = match spec.precision {
        Some(p) if p < bytes.len() => &bytes[..p],
        _ => bytes,
    };
    room(out, slice.len().max(spec.width))?;
    if slice.len() >= spec.width {
        out.extend_from_slice(slice);
        return Ok(());
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
    Ok(())
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
        args.push(decode_ref(raw));
    }

    // The literal bytes fit in the format string's own length; every
    // conversion reserves what it renders before it renders it.
    let mut out: Vec<u8> = Vec::new();
    if let Err(e) = room(&mut out, fmt_bytes.len() + 16) {
        return Some(Err(e));
    }
    // One conversion buffer reused across all args (stringify/digit helpers
    // clear + refill it) — no per-arg Vec churn on the device heap.
    let mut scratch: Vec<u8> = Vec::new();
    if let Err(e) = room(&mut scratch, 32) {
        return Some(Err(e));
    }
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
            b'%' | b'n' => {
                if let Err(e) = room(&mut out, 1) {
                    return Some(Err(e));
                }
                out.push(if spec.conv == b'%' { b'%' } else { b'\n' });
            }
            b's' | b'S' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                if let Err(e) = stringify(ctx, v, &mut scratch) {
                    return Some(Err(e));
                }
                if spec.conv == b'S' {
                    for c in scratch.iter_mut() {
                        c.make_ascii_uppercase();
                    }
                }
                if let Err(e) = pad_string(&scratch, &spec, &mut out) {
                    return Some(Err(e));
                }
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
                if let Err(e) = room(&mut scratch, s.len()) {
                    return Some(Err(e));
                }
                scratch.extend_from_slice(s);
                if spec.conv == b'B' {
                    for c in scratch.iter_mut() {
                        c.make_ascii_uppercase();
                    }
                }
                if let Err(e) = pad_string(&scratch, &spec, &mut out) {
                    return Some(Err(e));
                }
            }
            b'c' | b'C' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let ch = match unbox(ctx, v) {
                    Value::Int(n) if spec.precision.is_none() => n as u8,
                    _ => return Some(Err(fmt_err(ctx))),
                };
                scratch.clear();
                if let Err(e) = room(&mut scratch, 1) {
                    return Some(Err(e));
                }
                scratch.push(ch);
                if let Err(e) = pad_string(&scratch, &spec, &mut out) {
                    return Some(Err(e));
                }
            }
            b'd' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                // A precision is an IllegalFormatException on an integral
                // conversion, as in Java (QA 2026-09-13: `%.2d` was accepted).
                let (signed, _u) = match as_int(ctx, v) {
                    Some(t) if spec.precision.is_none() => t,
                    _ => return Some(Err(fmt_err(ctx))),
                };
                let neg = signed < 0;
                let mag = if neg {
                    (signed as i128).unsigned_abs() as u64
                } else {
                    signed as u64
                };
                if let Err(e) = decimal_digits(mag, spec.comma, &mut scratch) {
                    return Some(Err(e));
                }
                let sign = numeric_sign(neg, &spec);
                if let Err(e) = pad_numeric(sign, &scratch, &spec, &mut out) {
                    return Some(Err(e));
                }
            }
            b'x' | b'X' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let u = match as_unsigned(ctx, v) {
                    Some(u) if spec.precision.is_none() => u,
                    _ => return Some(Err(fmt_err(ctx))),
                };
                if let Err(e) = hex_digits(u, spec.conv == b'X', &mut scratch) {
                    return Some(Err(e));
                }
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
                if let Err(e) = pad_numeric(prefix, &scratch, &spec, &mut out) {
                    return Some(Err(e));
                }
            }
            b'o' => {
                if arg_pos >= args.len() {
                    return Some(Err(fmt_err(ctx)));
                }
                let v = args[arg_pos];
                arg_pos += 1;
                let u = match as_unsigned(ctx, v) {
                    Some(u) if spec.precision.is_none() => u,
                    _ => return Some(Err(fmt_err(ctx))),
                };
                if let Err(e) = oct_digits(u, &mut scratch) {
                    return Some(Err(e));
                }
                let prefix: &[u8] = if spec.hash && !scratch.starts_with(b"0") {
                    b"0"
                } else {
                    b""
                };
                if let Err(e) = pad_numeric(prefix, &scratch, &spec, &mut out) {
                    return Some(Err(e));
                }
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
                if !f.is_finite() {
                    // Java spells these out and pads with spaces even under
                    // the 0 flag; NaN never takes a sign.
                    let body: &[u8] = if f.is_nan() { b"NaN" } else { b"Infinity" };
                    let sign = if f.is_nan() {
                        b"" as &[u8]
                    } else {
                        numeric_sign(neg, &spec)
                    };
                    let spec_no_zero = Spec {
                        zero: false,
                        ..spec
                    };
                    if let Err(e) = pad_numeric(sign, body, &spec_no_zero, &mut out) {
                        return Some(Err(e));
                    }
                    continue;
                }
                // Java formats the shortest round-trip digits of the value
                // (`Double.toString`'s), rounded HALF_UP — not the exact
                // binary expansion rounded half-even, which is what Rust's
                // `{:.N}` prints. `%.2f` of 1.005 is `1.01`, `%.0f` of 2.5
                // is `3`, and 1.2345678901234567e22 under `%f` ends in
                // zeros (QA 2026-09-13).
                let (mut digits, n, point) = shortest_digits(mag);
                scratch.clear();
                let rendered = match spec.conv {
                    b'f' => fixed_body(&mut digits, n, point, prec, spec.comma, &mut scratch),
                    b'e' | b'E' => {
                        sci_body(&mut digits, n, point, prec, spec.conv == b'E', &mut scratch)
                    }
                    _ => {
                        // %g: HALF_UP to `prec` significant digits, then
                        // fixed notation for a result in [1e-4, 10^prec),
                        // scientific otherwise.
                        let p = if prec == 0 { 1 } else { prec };
                        let (rn, rp) = round_sig(&mut digits, n, point, p);
                        let exp = rp - 1;
                        if mag != 0.0 && (exp < -4 || exp >= p as i32) {
                            sci_body(&mut digits, rn, rp, p - 1, spec.conv == b'G', &mut scratch)
                        } else {
                            let frac = (p as i32 - rp).max(0) as usize;
                            fixed_body(&mut digits, rn, rp, frac, spec.comma, &mut scratch)
                        }
                    }
                };
                if let Err(e) = rendered {
                    return Some(Err(e));
                }
                let sign = numeric_sign(neg, &spec);
                if let Err(e) = pad_numeric(sign, &scratch, &spec, &mut out) {
                    return Some(Err(e));
                }
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

/// The shortest round-trip decimal digits of a finite `mag >= 0` — what
/// `Double.toString` prints — as `(digits, count, point)`:
/// `mag = 0.d₀d₁… × 10^point`, so `point` is the number of integer digits
/// (zero or negative below 0.1). At most 17 digits (the array leaves room
/// for a rounding carry); zero is `([0], 1, 1)`. No allocation.
fn shortest_digits(mag: f64) -> ([u8; 20], usize, i32) {
    use core::fmt::Write;
    let mut sb = StackBuf {
        buf: [0; 32],
        len: 0,
    };
    let _ = write!(sb, "{:e}", mag);
    let s = core::str::from_utf8(&sb.buf[..sb.len]).unwrap_or("0e0");
    let (mant, exp) = s.split_once('e').unwrap_or((s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let mut d = [0u8; 20];
    let mut n = 0;
    for b in mant.bytes() {
        if b.is_ascii_digit() && n < 20 {
            d[n] = b - b'0';
            n += 1;
        }
    }
    if n == 0 {
        n = 1;
    }
    (d, n, exp + 1)
}

/// Round the first `n` digits of `d` HALF_UP to `sig` significant digits
/// (`sig >= 1`), in place; returns the new count and point. A carry out of
/// the top digit lengthens the integer part (`9.99` → `10.0`).
fn round_sig(d: &mut [u8; 20], n: usize, point: i32, sig: usize) -> (usize, i32) {
    if n <= sig {
        return (n, point);
    }
    if d[sig] >= 5 {
        let mut i = sig;
        loop {
            if i == 0 {
                d[0] = 1;
                return (1, point + 1);
            }
            i -= 1;
            if d[i] == 9 {
                d[i] = 0;
            } else {
                d[i] += 1;
                break;
            }
        }
    }
    (sig, point)
}

/// `%.Nf`: the integer digits (grouped under the `,` flag), then `prec`
/// fractional digits, HALF_UP. Digits past the kept ones are implicit
/// zeros, so a 309-digit `%f` of 1e308 needs no buffer of its own.
fn fixed_body(
    d: &mut [u8; 20],
    n: usize,
    point: i32,
    prec: usize,
    comma: bool,
    out: &mut Vec<u8>,
) -> Result<(), JvmError> {
    let mut n = n;
    let mut point = point;
    // HALF_UP at fractional position `prec`: the first dropped digit is at
    // index point + prec of the digit string (an implicit zero outside it).
    let k = point as i64 + prec as i64;
    let first_dropped = if k >= 0 && (k as usize) < n {
        d[k as usize]
    } else {
        0
    };
    if first_dropped >= 5 {
        let mut i = k;
        loop {
            i -= 1;
            if i < 0 {
                // A new leading digit (0.96 at %.1f is 1.0): shift right.
                let m = n.min(19);
                for j in (0..m).rev() {
                    d[j + 1] = d[j];
                }
                d[0] = 1;
                n = m + 1;
                point += 1;
                break;
            }
            let iu = i as usize;
            if d[iu] == 9 {
                d[iu] = 0;
            } else {
                d[iu] += 1;
                break;
            }
        }
    }
    let k = point as i64 + prec as i64;
    if k < 0 {
        n = 0;
    } else if (k as usize) < n {
        n = k as usize;
    }
    let int_len = point.max(1) as usize;
    room(out, int_len + int_len / 3 + prec + 2)?;
    if point <= 0 {
        out.push(b'0');
    } else {
        for i in 0..int_len {
            if comma && i > 0 && (int_len - i) % 3 == 0 {
                out.push(b',');
            }
            out.push(b'0' + d.get(i).copied().filter(|_| i < n).unwrap_or(0));
        }
    }
    if prec > 0 {
        out.push(b'.');
        for i in 0..prec {
            let q = point as i64 + i as i64;
            out.push(
                b'0' + if q >= 0 && (q as usize) < n {
                    d[q as usize]
                } else {
                    0
                },
            );
        }
    }
    Ok(())
}

/// `%.Ne`: one integer digit, `prec` fractional digits, HALF_UP, and a
/// signed at-least-two-digit exponent (`1.234568e+04`).
fn sci_body(
    d: &mut [u8; 20],
    n: usize,
    point: i32,
    prec: usize,
    upper: bool,
    out: &mut Vec<u8>,
) -> Result<(), JvmError> {
    let zero = n == 1 && d[0] == 0;
    let (n, point) = round_sig(d, n, point, prec + 1);
    room(out, prec + 12)?;
    out.push(b'0' + d[0]);
    if prec > 0 {
        out.push(b'.');
        for i in 1..=prec {
            out.push(b'0' + d.get(i).copied().filter(|_| i < n).unwrap_or(0));
        }
    }
    out.push(if upper { b'E' } else { b'e' });
    let exp = if zero { 0 } else { point - 1 };
    out.push(if exp < 0 { b'-' } else { b'+' });
    let e = exp.unsigned_abs();
    if e < 10 {
        out.push(b'0');
    }
    let mut tmp = [0u8; 12];
    out.extend_from_slice(crate::object_heap::int_to_decimal_buf(e as i32, &mut tmp));
    Ok(())
}
