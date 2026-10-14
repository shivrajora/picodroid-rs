// SPDX-License-Identifier: GPL-3.0-only
//! Java-exact number-to-text: `Integer`/`Long.toString` digits and the
//! `Float`/`Double.toString` layout (JLS shortest-repr, `E` notation outside
//! 1e-3..1e7). Pure functions over caller buffers; no heap state.

/// The decimal digits of `m` ("0" for zero), right-aligned in `buf`;
/// returns where they start. The one `u64` digit loop: `Long.toString` and
/// `String.format`'s `%d` both use it. Nine digits come off per 64-bit
/// division and the rest is 32-bit arithmetic — the RP2040 has no hardware
/// divide, and a `u64 % 10` loop unrolled into a long run of
/// `__aeabi_uldivmod` calls in each copy.
#[inline(never)]
pub fn u64_to_decimal(mut m: u64, buf: &mut [u8; 21]) -> usize {
    let mut i = buf.len();
    loop {
        let mut chunk = (m % 1_000_000_000) as u32;
        m /= 1_000_000_000;
        if m == 0 {
            loop {
                i -= 1;
                buf[i] = b'0' + (chunk % 10) as u8;
                chunk /= 10;
                if chunk == 0 {
                    return i;
                }
            }
        }
        for _ in 0..9 {
            i -= 1;
            buf[i] = b'0' + (chunk % 10) as u8;
            chunk /= 10;
        }
    }
}

/// Format `n` as a decimal ASCII string into `buf`.  Returns the filled slice.
pub fn long_to_decimal_buf(n: i64, buf: &mut [u8; 21]) -> &[u8] {
    // Unsigned magnitude: `i64::MIN.wrapping_neg()` is still `i64::MIN`, so a
    // signed negate would leave the digit loop with nothing to emit.
    let mut i = u64_to_decimal(n.unsigned_abs(), buf);
    if n < 0 {
        i -= 1;
        buf[i] = b'-';
    }
    &buf[i..]
}

/// Format `n` as a decimal ASCII string into `buf`.  Returns the filled slice.
pub fn int_to_decimal_buf(n: i32, buf: &mut [u8; 12]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let neg = n < 0;
    // Unsigned magnitude — see `long_to_decimal_buf`.
    let mut m = n.unsigned_abs();
    let mut i = 12usize;
    while m > 0 {
        i -= 1;
        buf[i] = b'0' + (m % 10) as u8;
        m /= 10;
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    &buf[i..]
}

/// Java `Double.toString` / `Float.toString`: the shortest digit string that
/// round-trips, laid out as plain decimal with at least one fractional digit
/// for `1e-3 <= |x| < 1e7` and as computerized scientific notation
/// (`1.0E10`, `1.5E-5`) otherwise. Special values: "NaN", "Infinity",
/// "-Infinity".
///
/// The digits come from the exact-mode `{:.*e}` formatter at increasing
/// precision until the result parses back to `x` — the closest `p`-digit
/// decimal round-trips whenever any `p`-digit decimal does, so this is the
/// shortest representation. The precision starts at 1, not 0, which is
/// Java's rule (JDK 19+ `Double.toString`): when one digit would do, print
/// the closest decimal of one or two digits instead — `Double.MIN_VALUE`
/// is `4.9E-324`, not `5.0E-324`, and `Float.MIN_VALUE` is `1.4E-45`. The
/// two-digit candidate is never farther from `x` than the one-digit one,
/// so it round-trips whenever that does, and a trailing `0` is stripped.
///
/// Not a flash lever (measured 2026-10-14, flash-budget-2026-09.md §6.4):
/// on stable Rust every float `Display`/`LowerExp` call monomorphises both
/// the exact-mode and the shortest-mode printers and branches on the
/// `Formatter`'s precision at run time, so `String.format`'s `{:e}` keeps
/// the exact path linked whatever this function uses, and a version of
/// this on `{:e}` alone measured +3.6 KB (an `f32` instantiation, a scaled
/// two-digit rule). `str::parse` stays for `parseDouble` and pd-json.
pub(super) fn java_float_layout<'a>(
    d: f64,
    roundtrips: &dyn Fn(&str) -> bool,
    buf: &'a mut [u8; 32],
) -> &'a [u8] {
    struct W<'b> {
        buf: &'b mut [u8],
        len: usize,
    }
    impl W<'_> {
        fn push(&mut self, b: u8) {
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            }
        }
    }
    impl core::fmt::Write for W<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                self.push(b);
            }
            Ok(())
        }
    }
    use core::fmt::Write as _;

    if d.is_nan() {
        buf[..3].copy_from_slice(b"NaN");
        return &buf[..3];
    }
    if d.is_infinite() {
        let s: &[u8] = if d > 0.0 { b"Infinity" } else { b"-Infinity" };
        buf[..s.len()].copy_from_slice(s);
        return &buf[..s.len()];
    }
    let neg = d.is_sign_negative();
    let mag = d.abs();
    let mut out = W { buf, len: 0 };
    if neg {
        out.push(b'-');
    }
    if mag == 0.0 {
        out.write_str("0.0").ok();
        let len = out.len;
        return &out.buf[..len];
    }

    // Shortest round-tripping digits, as `d.ddd…e±N` from the exact formatter.
    let mut raw = [0u8; 32];
    let mut raw_len = 0;
    for prec in 1..17 {
        let mut w = W {
            buf: &mut raw,
            len: 0,
        };
        let _ = write!(w, "{mag:.prec$e}");
        raw_len = w.len;
        let text = core::str::from_utf8(&raw[..raw_len]).unwrap_or("");
        if roundtrips(text) {
            break;
        }
    }
    let raw = &raw[..raw_len];
    let epos = raw.iter().position(|&b| b == b'e').unwrap_or(raw_len);
    let mut digits = [b'0'; 20];
    let mut nd = 0;
    for &b in &raw[..epos] {
        if b != b'.' && nd < digits.len() {
            digits[nd] = b;
            nd += 1;
        }
    }
    while nd > 1 && digits[nd - 1] == b'0' {
        nd -= 1;
    }
    let mut exp: i32 = 0;
    let mut exp_neg = false;
    for &b in &raw[(epos + 1).min(raw_len)..] {
        match b {
            b'-' => exp_neg = true,
            b'0'..=b'9' => exp = exp * 10 + (b - b'0') as i32,
            _ => {}
        }
    }
    if exp_neg {
        exp = -exp;
    }

    if !(1e-3..1e7).contains(&mag) {
        // d.dddE±N — at least one fractional digit.
        out.push(digits[0]);
        out.push(b'.');
        if nd > 1 {
            for &b in &digits[1..nd] {
                out.push(b);
            }
        } else {
            out.push(b'0');
        }
        out.push(b'E');
        let _ = write!(out, "{exp}");
    } else if exp >= 0 {
        let int_len = exp as usize + 1;
        let head = &digits[..nd];
        for i in 0..int_len {
            out.push(head.get(i).copied().unwrap_or(b'0'));
        }
        out.push(b'.');
        if nd > int_len {
            for &b in &digits[int_len..nd] {
                out.push(b);
            }
        } else {
            out.push(b'0');
        }
    } else {
        out.write_str("0.").ok();
        for _ in 0..(-exp - 1) {
            out.push(b'0');
        }
        for &b in &digits[..nd] {
            out.push(b);
        }
    }
    let len = out.len;
    &out.buf[..len]
}

/// Format `d` as `java.lang.Double.toString` would. See [`java_float_layout`].
pub fn double_to_str_buf(d: f64, buf: &mut [u8; 32]) -> &[u8] {
    java_float_layout(d, &|s| s.parse::<f64>().ok() == Some(d), buf)
}

/// Format `f` as `java.lang.Float.toString` would. See [`java_float_layout`].
pub fn float_to_str_buf(f: f32, buf: &mut [u8; 32]) -> &[u8] {
    // Formatting goes through f64 (exact widening) and so does the parse —
    // `str::parse::<f32>` would link a second dec2flt instantiation (RP2040
    // flash). The double rounding (decimal -> f64 -> f32) can only differ
    // from a direct f32 parse for a decimal within an f64 ulp of an f32
    // midpoint, which a <= 9-digit candidate never is.
    java_float_layout(
        f as f64,
        &|s| s.parse::<f64>().ok().map(|d| d as f32) == Some(f),
        buf,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn long_to_decimal_matches_display_across_chunk_boundaries() {
        let mut cases = alloc::vec![i64::MIN, i64::MIN + 1, i64::MAX, -1, 0, 1];
        for p in 0..19 {
            let t = 10i64.pow(p);
            cases.extend([t - 1, t, t + 1, -(t - 1), -t, -(t + 1)]);
        }
        for n in cases {
            let mut buf = [0u8; 21];
            assert_eq!(long_to_decimal_buf(n, &mut buf), n.to_string().as_bytes());
        }
        let mut buf = [0u8; 21];
        let i = u64_to_decimal(u64::MAX, &mut buf);
        assert_eq!(&buf[i..], u64::MAX.to_string().as_bytes());
    }
}
