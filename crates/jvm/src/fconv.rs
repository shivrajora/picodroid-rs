//! Java's floating-point to integral conversions (JLS §5.1.3): truncate
//! toward zero, NaN to 0, out-of-range values saturating.
//!
//! Rust's `as` has exactly these semantics — on every target whose
//! conversion intrinsic does. On the RP2040 it does not: rp2040-hal maps
//! `__aeabi_f2iz` / `f2lz` / `d2iz` / `d2lz` to the bootrom's
//! `float_to_int` family, which rounds toward −∞, so `(int) -0.9f` was -1
//! there (QA 2026-09-13, qa_lang on testbench_rp2040; the FPU boards and
//! the sim truncate). Truncating first means the cast only ever sees an
//! integral value, whichever rounding the platform's intrinsic applies.

/// `f2i`.
#[inline]
pub fn f2i(f: f32) -> i32 {
    libm::truncf(f) as i32
}

/// `f2l`.
#[inline]
pub fn f2l(f: f32) -> i64 {
    libm::truncf(f) as i64
}

/// `d2i`.
#[inline]
pub fn d2i(d: f64) -> i32 {
    libm::trunc(d) as i32
}

/// `d2l`.
#[inline]
pub fn d2l(d: f64) -> i64 {
    libm::trunc(d) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_toward_zero_and_saturates() {
        assert_eq!(f2i(-0.9), 0);
        assert_eq!(f2i(-1.5), -1);
        assert_eq!(f2i(2.9), 2);
        assert_eq!(d2i(-0.9), 0);
        assert_eq!(d2l(-1.999), -1);
        assert_eq!(f2l(-0.5), 0);
        assert_eq!(f2i(f32::NAN), 0);
        assert_eq!(d2l(f64::NAN), 0);
        assert_eq!(f2i(f32::INFINITY), i32::MAX);
        assert_eq!(f2i(f32::NEG_INFINITY), i32::MIN);
        assert_eq!(d2i(1e300), i32::MAX);
        assert_eq!(d2l(-1e300), i64::MIN);
        assert_eq!(f2l(1e30), i64::MAX);
    }
}
