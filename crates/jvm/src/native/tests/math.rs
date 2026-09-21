// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── abs ──────────────────────────────────────────────────────────────────

#[test]
fn abs_min_value_is_min_value() {
    // Java: Math.abs(Integer.MIN_VALUE) == Integer.MIN_VALUE (no exception,
    // no panic). `i32::abs` overflows under debug overflow checks.
    assert_eq!(
        dispatch_math(m::abs, "(I)I", &[Value::Int(i32::MIN)]),
        Ok(Some(Value::Int(i32::MIN)))
    );
    assert_eq!(
        dispatch_math(m::abs, "(J)J", &[Value::Long(i64::MIN)]),
        Ok(Some(Value::Long(i64::MIN)))
    );
}

#[test]
fn round_negative_half_rounds_toward_positive_infinity() {
    // Java's Math.round is floor(x + 0.5): -2.5 -> -2, -0.5 -> 0, 2.5 -> 3.
    assert_eq!(
        dispatch_math(m::round, "(F)I", &[Value::Float(-2.5)]),
        Ok(Some(Value::Int(-2)))
    );
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(-2.5)]),
        Ok(Some(Value::Long(-2)))
    );
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(-0.5)]),
        Ok(Some(Value::Long(0)))
    );
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(2.5)]),
        Ok(Some(Value::Long(3)))
    );
    // NaN -> 0, saturation at the integer range.
    assert_eq!(
        dispatch_math(m::round, "(F)I", &[Value::Float(f32::NAN)]),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(1e30)]),
        Ok(Some(Value::Long(i64::MAX)))
    );
}

#[test]
fn min_max_propagate_nan_and_order_signed_zero() {
    // Java: NaN if either argument is NaN; -0.0 < 0.0.
    let r = dispatch_math(
        m::min,
        "(DD)D",
        &[Value::Double(f64::NAN), Value::Double(1.0)],
    );
    assert!(
        matches!(r, Ok(Some(Value::Double(d))) if d.is_nan()),
        "{r:?}"
    );
    let r = dispatch_math(
        m::max,
        "(FF)F",
        &[Value::Float(1.0), Value::Float(f32::NAN)],
    );
    assert!(
        matches!(r, Ok(Some(Value::Float(f))) if f.is_nan()),
        "{r:?}"
    );
    let r = dispatch_math(m::min, "(DD)D", &[Value::Double(0.0), Value::Double(-0.0)]);
    assert!(
        matches!(r, Ok(Some(Value::Double(d))) if d == 0.0 && d.is_sign_negative()),
        "{r:?}"
    );
    let r = dispatch_math(m::max, "(DD)D", &[Value::Double(-0.0), Value::Double(0.0)]);
    assert!(
        matches!(r, Ok(Some(Value::Double(d))) if d == 0.0 && d.is_sign_positive()),
        "{r:?}"
    );
}

#[test]
fn abs_int_positive() {
    assert_eq!(
        dispatch_math(m::abs, "(I)I", &[Value::Int(5)]),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn abs_int_negative() {
    assert_eq!(
        dispatch_math(m::abs, "(I)I", &[Value::Int(-5)]),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn abs_long_negative() {
    assert_eq!(
        dispatch_math(m::abs, "(J)J", &[Value::Long(-10)]),
        Ok(Some(Value::Long(10)))
    );
}

#[test]
fn abs_float_negative() {
    assert_eq!(
        dispatch_math(m::abs, "(F)F", &[Value::Float(-3.5)]),
        Ok(Some(Value::Float(3.5)))
    );
}

#[test]
fn abs_double_negative() {
    assert_eq!(
        dispatch_math(m::abs, "(D)D", &[Value::Double(-2.0)]),
        Ok(Some(Value::Double(2.0)))
    );
}

// ── min ──────────────────────────────────────────────────────────────────

#[test]
fn min_int() {
    assert_eq!(
        dispatch_math(m::min, "(II)I", &[Value::Int(3), Value::Int(7)]),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn min_long() {
    assert_eq!(
        dispatch_math(m::min, "(JJ)J", &[Value::Long(100), Value::Long(50)]),
        Ok(Some(Value::Long(50)))
    );
}

#[test]
fn min_float() {
    assert_eq!(
        dispatch_math(m::min, "(FF)F", &[Value::Float(1.5), Value::Float(2.5)]),
        Ok(Some(Value::Float(1.5)))
    );
}

#[test]
fn min_double() {
    assert_eq!(
        dispatch_math(m::min, "(DD)D", &[Value::Double(0.1), Value::Double(0.2)]),
        Ok(Some(Value::Double(0.1)))
    );
}

// ── max ──────────────────────────────────────────────────────────────────

#[test]
fn max_int() {
    assert_eq!(
        dispatch_math(m::max, "(II)I", &[Value::Int(3), Value::Int(7)]),
        Ok(Some(Value::Int(7)))
    );
}

#[test]
fn max_long() {
    assert_eq!(
        dispatch_math(m::max, "(JJ)J", &[Value::Long(100), Value::Long(50)]),
        Ok(Some(Value::Long(100)))
    );
}

#[test]
fn max_float() {
    assert_eq!(
        dispatch_math(m::max, "(FF)F", &[Value::Float(1.5), Value::Float(2.5)]),
        Ok(Some(Value::Float(2.5)))
    );
}

#[test]
fn max_double() {
    assert_eq!(
        dispatch_math(m::max, "(DD)D", &[Value::Double(9.0), Value::Double(3.0)]),
        Ok(Some(Value::Double(9.0)))
    );
}

// ── sqrt ─────────────────────────────────────────────────────────────────

#[test]
fn sqrt_four() {
    assert_eq!(
        dispatch_math(m::sqrt, "(D)D", &[Value::Double(4.0)]),
        Ok(Some(Value::Double(2.0)))
    );
}

#[test]
fn sqrt_two() {
    let Value::Double(result) = dispatch_math(m::sqrt, "(D)D", &[Value::Double(2.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - 1.4142135).abs() < 1e-6);
}

// ── pow ──────────────────────────────────────────────────────────────────

#[test]
fn pow_two_ten() {
    assert_eq!(
        dispatch_math(m::pow, "(DD)D", &[Value::Double(2.0), Value::Double(10.0)]),
        Ok(Some(Value::Double(1024.0)))
    );
}

// ── floor / ceil ─────────────────────────────────────────────────────────

#[test]
fn floor_positive() {
    assert_eq!(
        dispatch_math(m::floor, "(D)D", &[Value::Double(2.9)]),
        Ok(Some(Value::Double(2.0)))
    );
}

#[test]
fn floor_negative() {
    assert_eq!(
        dispatch_math(m::floor, "(D)D", &[Value::Double(-2.1)]),
        Ok(Some(Value::Double(-3.0)))
    );
}

#[test]
fn ceil_positive() {
    assert_eq!(
        dispatch_math(m::ceil, "(D)D", &[Value::Double(2.1)]),
        Ok(Some(Value::Double(3.0)))
    );
}

#[test]
fn ceil_negative() {
    assert_eq!(
        dispatch_math(m::ceil, "(D)D", &[Value::Double(-2.9)]),
        Ok(Some(Value::Double(-2.0)))
    );
}

// ── round ────────────────────────────────────────────────────────────────

#[test]
fn round_float_up() {
    assert_eq!(
        dispatch_math(m::round, "(F)I", &[Value::Float(2.6)]),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn round_float_down() {
    assert_eq!(
        dispatch_math(m::round, "(F)I", &[Value::Float(2.4)]),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn round_double() {
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(2.5)]),
        Ok(Some(Value::Long(3)))
    );
}

// ── sin / cos / tan ───────────────────────────────────────────────────────

#[test]
fn sin_zero() {
    assert_eq!(
        dispatch_math(m::sin, "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(0.0)))
    );
}

#[test]
fn cos_zero() {
    assert_eq!(
        dispatch_math(m::cos, "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(1.0)))
    );
}

#[test]
fn sin_pi_over_2() {
    let Value::Double(result) = dispatch_math(
        m::sin,
        "(D)D",
        &[Value::Double(core::f64::consts::FRAC_PI_2)],
    )
    .unwrap()
    .unwrap() else {
        panic!("expected Double");
    };
    assert!((result - 1.0).abs() < 1e-10);
}

#[test]
fn tan_zero() {
    assert_eq!(
        dispatch_math(m::tan, "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(0.0)))
    );
}

// ── atan2 ────────────────────────────────────────────────────────────────

#[test]
fn atan2_one_one() {
    let Value::Double(result) =
        dispatch_math(m::atan2, "(DD)D", &[Value::Double(1.0), Value::Double(1.0)])
            .unwrap()
            .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - core::f64::consts::FRAC_PI_4).abs() < 1e-10);
}

// ── toRadians / toDegrees ────────────────────────────────────────────────

#[test]
fn to_radians_180() {
    let Value::Double(result) = dispatch_math(m::toRadians, "(D)D", &[Value::Double(180.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - core::f64::consts::PI).abs() < 1e-10);
}

#[test]
fn to_degrees_pi() {
    let Value::Double(result) = dispatch_math(
        m::toDegrees,
        "(D)D",
        &[Value::Double(core::f64::consts::PI)],
    )
    .unwrap()
    .unwrap() else {
        panic!("expected Double");
    };
    assert!((result - 180.0).abs() < 1e-10);
}

// ── log / log10 / exp ────────────────────────────────────────────────────

#[test]
fn log_e() {
    let Value::Double(result) =
        dispatch_math(m::log, "(D)D", &[Value::Double(core::f64::consts::E)])
            .unwrap()
            .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - 1.0).abs() < 1e-10);
}

#[test]
fn log10_100() {
    let Value::Double(result) = dispatch_math(m::log10, "(D)D", &[Value::Double(100.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - 2.0).abs() < 1e-10);
}

#[test]
fn exp_zero() {
    assert_eq!(
        dispatch_math(m::exp, "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(1.0)))
    );
}

#[test]
fn exp_one() {
    let Value::Double(result) = dispatch_math(m::exp, "(D)D", &[Value::Double(1.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - core::f64::consts::E).abs() < 1e-10);
}
