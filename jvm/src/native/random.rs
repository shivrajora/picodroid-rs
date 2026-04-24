use core::sync::atomic::{AtomicU32, Ordering};

use crate::{
    array_heap::ATYPE_BYTE,
    types::{JvmError, Value},
};

use super::NativeContext;

/// Java's `Random` LCG: `seed = (seed * 0x5DEECE66D + 0xB) & ((1 << 48) - 1)`.
const MULTIPLIER: i64 = 0x5DEECE66D;
const ADDEND: i64 = 0xB;
const MASK: i64 = (1i64 << 48) - 1;

/// Counter used to derive default seeds when `new Random()` is called with no
/// argument. 32-bit because thumbv6m (RP2040) lacks 64-bit atomics; the
/// load/store pair is non-atomic across threads but the resulting seed is
/// still valid pseudorandom — `Random` is non-cryptographic, and the
/// worst-case race is two instances getting the same seed.
static SEED_UNIQUIFIER: AtomicU32 = AtomicU32::new(0x9E37_79B9);

fn next_uniquifier() -> i64 {
    // Multiplier mirrors the JDK's `seedUniquifier` shape (truncated to 32 bits).
    let prev = SEED_UNIQUIFIER.load(Ordering::Relaxed);
    let next = prev.wrapping_mul(0x6D2B_79F5);
    SEED_UNIQUIFIER.store(next, Ordering::Relaxed);
    next as i64
}

fn scramble(seed: i64) -> i64 {
    (seed ^ MULTIPLIER) & MASK
}

fn read_seed(ctx: &NativeContext<'_>, this: u16) -> Result<i64, JvmError> {
    match ctx.objects.get_field(this, 0) {
        Some(Value::Long(s)) => Ok(s),
        _ => Err(JvmError::InvalidReference),
    }
}

fn write_seed(ctx: &mut NativeContext<'_>, this: u16, seed: i64) {
    ctx.objects.set_field(this, 0, Value::Long(seed));
}

/// Single LCG step. Returns `(new_seed, top `bits` bits as i32)`.
fn step(seed: i64, bits: u32) -> (i64, i32) {
    let new_seed = (seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND)) & MASK;
    let n = (new_seed as u64 >> (48 - bits)) as i32;
    (new_seed, n)
}

/// Advance the seed in field 0 once and return the top `bits` bits.
fn next_bits(ctx: &mut NativeContext<'_>, this: u16, bits: u32) -> Result<i32, JvmError> {
    let s = read_seed(ctx, this)?;
    let (s2, n) = step(s, bits);
    write_seed(ctx, this, s2);
    Ok(n)
}

fn extract_this(args: &[Value]) -> Result<u16, JvmError> {
    match args.first().copied().unwrap_or(Value::Null) {
        Value::ObjectRef(i) => Ok(i),
        _ => Err(JvmError::InvalidReference),
    }
}

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            // Two overloads: <init>() and <init>(J).
            let seed = if ctx.descriptor.starts_with("(J)") {
                match ctx.args.get(1).copied().unwrap_or(Value::Null) {
                    Value::Long(s) => scramble(s),
                    _ => return Some(Err(JvmError::InvalidReference)),
                }
            } else {
                scramble(next_uniquifier())
            };
            write_seed(ctx, this, seed);
            // Field 1 holds a cached gaussian (Value::Double) or Value::Null.
            ctx.objects.set_field(this, 1, Value::Null);
            Some(Ok(None))
        }
        "setSeed" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            match ctx.args.get(1).copied().unwrap_or(Value::Null) {
                Value::Long(s) => {
                    write_seed(ctx, this, scramble(s));
                    ctx.objects.set_field(this, 1, Value::Null);
                    Some(Ok(None))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            }
        }
        "nextInt" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            // nextInt() vs nextInt(int bound).
            if ctx.descriptor.starts_with("()") {
                match next_bits(ctx, this, 32) {
                    Ok(n) => Some(Ok(Some(Value::Int(n)))),
                    Err(e) => Some(Err(e)),
                }
            } else {
                let bound = match ctx.args.get(1).copied().unwrap_or(Value::Null) {
                    Value::Int(b) => b,
                    _ => return Some(Err(JvmError::InvalidReference)),
                };
                if bound <= 0 {
                    return Some(Err(JvmError::InvalidReference));
                }
                // Power-of-2 fast path matches the JDK.
                if (bound & -bound) == bound {
                    return match next_bits(ctx, this, 31) {
                        Ok(n) => {
                            let r = ((bound as i64).wrapping_mul(n as i64) >> 31) as i32;
                            Some(Ok(Some(Value::Int(r))))
                        }
                        Err(e) => Some(Err(e)),
                    };
                }
                // Rejection sampling for unbiased result.
                loop {
                    let bits = match next_bits(ctx, this, 31) {
                        Ok(n) => n,
                        Err(e) => return Some(Err(e)),
                    };
                    let val = bits % bound;
                    if bits.wrapping_sub(val).wrapping_add(bound - 1) >= 0 {
                        return Some(Ok(Some(Value::Int(val))));
                    }
                }
            }
        }
        "nextLong" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let hi = match next_bits(ctx, this, 32) {
                Ok(n) => n as i64,
                Err(e) => return Some(Err(e)),
            };
            let lo = match next_bits(ctx, this, 32) {
                Ok(n) => n as i64,
                Err(e) => return Some(Err(e)),
            };
            // JDK uses `((long)next(32) << 32) + next(32)` — sign-extends low
            // half via `(int) -> long` cast, so we mirror that with a wrap.
            Some(Ok(Some(Value::Long((hi << 32).wrapping_add(lo)))))
        }
        "nextBoolean" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            match next_bits(ctx, this, 1) {
                Ok(n) => Some(Ok(Some(Value::Int((n != 0) as i32)))),
                Err(e) => Some(Err(e)),
            }
        }
        "nextFloat" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            match next_bits(ctx, this, 24) {
                Ok(n) => Some(Ok(Some(Value::Float(n as f32 / (1u32 << 24) as f32)))),
                Err(e) => Some(Err(e)),
            }
        }
        "nextDouble" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let hi = match next_bits(ctx, this, 26) {
                Ok(n) => n as i64,
                Err(e) => return Some(Err(e)),
            };
            let lo = match next_bits(ctx, this, 27) {
                Ok(n) => n as i64,
                Err(e) => return Some(Err(e)),
            };
            // (((long)next(26) << 27) + next(27)) / (double)(1L << 53)
            let bits = (hi << 27) + lo;
            Some(Ok(Some(Value::Double(bits as f64 / (1u64 << 53) as f64))))
        }
        "nextGaussian" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            // If a value was cached on the previous call, return it and clear.
            if let Some(Value::Double(d)) = ctx.objects.get_field(this, 1) {
                ctx.objects.set_field(this, 1, Value::Null);
                return Some(Ok(Some(Value::Double(d))));
            }
            // Marsaglia polar method (matches the JDK implementation).
            loop {
                let u1 = match next_double(ctx, this) {
                    Ok(d) => d,
                    Err(e) => return Some(Err(e)),
                };
                let u2 = match next_double(ctx, this) {
                    Ok(d) => d,
                    Err(e) => return Some(Err(e)),
                };
                let v1 = 2.0 * u1 - 1.0;
                let v2 = 2.0 * u2 - 1.0;
                let s = v1 * v1 + v2 * v2;
                if s >= 1.0 || s == 0.0 {
                    continue;
                }
                let multiplier = libm::sqrt(-2.0 * libm::log(s) / s);
                // Cache v2 * multiplier; return v1 * multiplier.
                ctx.objects
                    .set_field(this, 1, Value::Double(v2 * multiplier));
                return Some(Ok(Some(Value::Double(v1 * multiplier))));
            }
        }
        "nextBytes" => {
            let this = match extract_this(ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let arr_idx = match ctx.args.get(1).copied().unwrap_or(Value::Null) {
                Value::ArrayRef(i) => i,
                Value::Null => return Some(Err(JvmError::InvalidReference)),
                _ => return Some(Err(JvmError::InvalidReference)),
            };
            // Verify it really is a byte[].
            if ctx.arrays.atype(arr_idx) != Some(ATYPE_BYTE) {
                return Some(Err(JvmError::InvalidReference));
            }
            let len = match ctx.arrays.length(arr_idx) {
                Some(l) => l as usize,
                None => return Some(Err(JvmError::InvalidReference)),
            };
            // Match JDK pattern: pull 4 bytes per next(32) call.
            let mut i = 0usize;
            while i < len {
                let mut rnd = match next_bits(ctx, this, 32) {
                    Ok(n) => n,
                    Err(e) => return Some(Err(e)),
                };
                let n = core::cmp::min(len - i, 4);
                for _ in 0..n {
                    // Sign-extend each byte to i32 like Java does on byte[] write.
                    let byte = (rnd as i8) as i32;
                    if ctx.arrays.store(arr_idx, i, byte).is_none() {
                        return Some(Err(JvmError::ArrayIndexOutOfBounds));
                    }
                    rnd >>= 8;
                    i += 1;
                }
            }
            Some(Ok(None))
        }
        _ => None,
    }
}

/// Internal helper mirroring `nextDouble` for use inside `nextGaussian`.
fn next_double(ctx: &mut NativeContext<'_>, this: u16) -> Result<f64, JvmError> {
    let hi = next_bits(ctx, this, 26)? as i64;
    let lo = next_bits(ctx, this, 27)? as i64;
    let bits = (hi << 27) + lo;
    Ok(bits as f64 / (1u64 << 53) as f64)
}
