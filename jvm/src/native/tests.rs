use super::*;
use crate::{array_heap::ArrayHeap, heap::StringTable, object_heap::ObjectHeap};

fn dispatch_math(
    method: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        descriptor,
        args,
        strings: &mut strings,
        objects: &mut objects,
        arrays: &mut arrays,
    };
    BuiltinHandler
        .dispatch("java/lang/Math", method, &mut ctx)
        .expect("Math method not handled")
}

// ── abs ──────────────────────────────────────────────────────────────────

#[test]
fn abs_int_positive() {
    assert_eq!(
        dispatch_math("abs", "(I)I", &[Value::Int(5)]),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn abs_int_negative() {
    assert_eq!(
        dispatch_math("abs", "(I)I", &[Value::Int(-5)]),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn abs_long_negative() {
    assert_eq!(
        dispatch_math("abs", "(J)J", &[Value::Long(-10)]),
        Ok(Some(Value::Long(10)))
    );
}

#[test]
fn abs_float_negative() {
    assert_eq!(
        dispatch_math("abs", "(F)F", &[Value::Float(-3.5)]),
        Ok(Some(Value::Float(3.5)))
    );
}

#[test]
fn abs_double_negative() {
    assert_eq!(
        dispatch_math("abs", "(D)D", &[Value::Double(-2.0)]),
        Ok(Some(Value::Double(2.0)))
    );
}

// ── min ──────────────────────────────────────────────────────────────────

#[test]
fn min_int() {
    assert_eq!(
        dispatch_math("min", "(II)I", &[Value::Int(3), Value::Int(7)]),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn min_long() {
    assert_eq!(
        dispatch_math("min", "(JJ)J", &[Value::Long(100), Value::Long(50)]),
        Ok(Some(Value::Long(50)))
    );
}

#[test]
fn min_float() {
    assert_eq!(
        dispatch_math("min", "(FF)F", &[Value::Float(1.5), Value::Float(2.5)]),
        Ok(Some(Value::Float(1.5)))
    );
}

#[test]
fn min_double() {
    assert_eq!(
        dispatch_math("min", "(DD)D", &[Value::Double(0.1), Value::Double(0.2)]),
        Ok(Some(Value::Double(0.1)))
    );
}

// ── max ──────────────────────────────────────────────────────────────────

#[test]
fn max_int() {
    assert_eq!(
        dispatch_math("max", "(II)I", &[Value::Int(3), Value::Int(7)]),
        Ok(Some(Value::Int(7)))
    );
}

#[test]
fn max_long() {
    assert_eq!(
        dispatch_math("max", "(JJ)J", &[Value::Long(100), Value::Long(50)]),
        Ok(Some(Value::Long(100)))
    );
}

#[test]
fn max_float() {
    assert_eq!(
        dispatch_math("max", "(FF)F", &[Value::Float(1.5), Value::Float(2.5)]),
        Ok(Some(Value::Float(2.5)))
    );
}

#[test]
fn max_double() {
    assert_eq!(
        dispatch_math("max", "(DD)D", &[Value::Double(9.0), Value::Double(3.0)]),
        Ok(Some(Value::Double(9.0)))
    );
}

// ── sqrt ─────────────────────────────────────────────────────────────────

#[test]
fn sqrt_four() {
    assert_eq!(
        dispatch_math("sqrt", "(D)D", &[Value::Double(4.0)]),
        Ok(Some(Value::Double(2.0)))
    );
}

#[test]
fn sqrt_two() {
    let Value::Double(result) = dispatch_math("sqrt", "(D)D", &[Value::Double(2.0)])
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
        dispatch_math("pow", "(DD)D", &[Value::Double(2.0), Value::Double(10.0)]),
        Ok(Some(Value::Double(1024.0)))
    );
}

// ── floor / ceil ─────────────────────────────────────────────────────────

#[test]
fn floor_positive() {
    assert_eq!(
        dispatch_math("floor", "(D)D", &[Value::Double(2.9)]),
        Ok(Some(Value::Double(2.0)))
    );
}

#[test]
fn floor_negative() {
    assert_eq!(
        dispatch_math("floor", "(D)D", &[Value::Double(-2.1)]),
        Ok(Some(Value::Double(-3.0)))
    );
}

#[test]
fn ceil_positive() {
    assert_eq!(
        dispatch_math("ceil", "(D)D", &[Value::Double(2.1)]),
        Ok(Some(Value::Double(3.0)))
    );
}

#[test]
fn ceil_negative() {
    assert_eq!(
        dispatch_math("ceil", "(D)D", &[Value::Double(-2.9)]),
        Ok(Some(Value::Double(-2.0)))
    );
}

// ── round ────────────────────────────────────────────────────────────────

#[test]
fn round_float_up() {
    assert_eq!(
        dispatch_math("round", "(F)I", &[Value::Float(2.6)]),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn round_float_down() {
    assert_eq!(
        dispatch_math("round", "(F)I", &[Value::Float(2.4)]),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn round_double() {
    assert_eq!(
        dispatch_math("round", "(D)J", &[Value::Double(2.5)]),
        Ok(Some(Value::Long(3)))
    );
}

// ── sin / cos / tan ───────────────────────────────────────────────────────

#[test]
fn sin_zero() {
    assert_eq!(
        dispatch_math("sin", "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(0.0)))
    );
}

#[test]
fn cos_zero() {
    assert_eq!(
        dispatch_math("cos", "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(1.0)))
    );
}

#[test]
fn sin_pi_over_2() {
    let Value::Double(result) = dispatch_math(
        "sin",
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
        dispatch_math("tan", "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(0.0)))
    );
}

// ── atan2 ────────────────────────────────────────────────────────────────

#[test]
fn atan2_one_one() {
    let Value::Double(result) =
        dispatch_math("atan2", "(DD)D", &[Value::Double(1.0), Value::Double(1.0)])
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
    let Value::Double(result) = dispatch_math("toRadians", "(D)D", &[Value::Double(180.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - core::f64::consts::PI).abs() < 1e-10);
}

#[test]
fn to_degrees_pi() {
    let Value::Double(result) =
        dispatch_math("toDegrees", "(D)D", &[Value::Double(core::f64::consts::PI)])
            .unwrap()
            .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - 180.0).abs() < 1e-10);
}

// ── log / log10 / exp ────────────────────────────────────────────────────

#[test]
fn log_e() {
    let Value::Double(result) =
        dispatch_math("log", "(D)D", &[Value::Double(core::f64::consts::E)])
            .unwrap()
            .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - 1.0).abs() < 1e-10);
}

#[test]
fn log10_100() {
    let Value::Double(result) = dispatch_math("log10", "(D)D", &[Value::Double(100.0)])
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
        dispatch_math("exp", "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(1.0)))
    );
}

#[test]
fn exp_one() {
    let Value::Double(result) = dispatch_math("exp", "(D)D", &[Value::Double(1.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - core::f64::consts::E).abs() < 1e-10);
}
