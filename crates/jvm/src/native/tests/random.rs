// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── Random native method tests ───────────────────────────────────────────

#[test]
fn random_seed_determinism_int() {
    let mut a = RngCtx::new(42);
    let mut b = RngCtx::new(42);
    for _ in 0..16 {
        assert_eq!(
            a.call(m::nextInt, "()I", &[]),
            b.call(m::nextInt, "()I", &[])
        );
    }
}

#[test]
fn random_seed_determinism_long() {
    let mut a = RngCtx::new(0xCAFE_BABEi64);
    let mut b = RngCtx::new(0xCAFE_BABEi64);
    for _ in 0..16 {
        assert_eq!(
            a.call(m::nextLong, "()J", &[]),
            b.call(m::nextLong, "()J", &[])
        );
    }
}

#[test]
fn random_setseed_resets_sequence() {
    let mut r = RngCtx::new(7);
    let first = r.call(m::nextInt, "()I", &[]);
    // Re-seed with the same value; next draw must match the first draw.
    r.call(m::setSeed, "(J)V", &[Value::Long(7)]);
    assert_eq!(r.call(m::nextInt, "()I", &[]), first);
}

#[test]
fn random_next_int_bound_in_range() {
    let mut r = RngCtx::new(123);
    for _ in 0..256 {
        let v = r.call(m::nextInt, "(I)I", &[Value::Int(10)]);
        match v {
            Some(Value::Int(n)) => assert!((0..10).contains(&n), "out of range: {n}"),
            other => panic!("expected Int, got {other:?}"),
        }
    }
}

#[test]
fn random_next_int_bound_power_of_two() {
    // Exercises the JDK's bound-is-power-of-2 fast path.
    let mut r = RngCtx::new(99);
    for _ in 0..256 {
        let v = r.call(m::nextInt, "(I)I", &[Value::Int(64)]);
        match v {
            Some(Value::Int(n)) => assert!((0..64).contains(&n), "out of range: {n}"),
            other => panic!("expected Int, got {other:?}"),
        }
    }
}

#[test]
fn random_next_float_in_unit_interval() {
    let mut r = RngCtx::new(1);
    for _ in 0..64 {
        match r.call(m::nextFloat, "()F", &[]) {
            Some(Value::Float(f)) => assert!((0.0..1.0).contains(&f), "out of [0,1): {f}"),
            other => panic!("expected Float, got {other:?}"),
        }
    }
}

#[test]
fn random_next_double_in_unit_interval() {
    let mut r = RngCtx::new(2);
    for _ in 0..64 {
        match r.call(m::nextDouble, "()D", &[]) {
            Some(Value::Double(d)) => assert!((0.0..1.0).contains(&d), "out of [0,1): {d}"),
            other => panic!("expected Double, got {other:?}"),
        }
    }
}

#[test]
fn random_next_boolean_yields_both_values() {
    let mut r = RngCtx::new(3);
    let mut saw_true = false;
    let mut saw_false = false;
    for _ in 0..64 {
        match r.call(m::nextBoolean, "()Z", &[]) {
            Some(Value::Int(0)) => saw_false = true,
            Some(Value::Int(1)) => saw_true = true,
            other => panic!("expected boolean Int, got {other:?}"),
        }
    }
    assert!(
        saw_true && saw_false,
        "boolean RNG biased: t={saw_true} f={saw_false}"
    );
}

#[test]
fn random_next_gaussian_distribution_sanity() {
    // Marsaglia polar with 256 samples — mean within ±0.3, stddev within ±0.3 of 1.
    let mut r = RngCtx::new(0xDEAD_BEEFi64);
    let n = 256usize;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    for _ in 0..n {
        match r.call(m::nextGaussian, "()D", &[]) {
            Some(Value::Double(d)) => {
                sum += d;
                sum_sq += d * d;
            }
            other => panic!("expected Double, got {other:?}"),
        }
    }
    let mean = sum / n as f64;
    let variance = sum_sq / n as f64 - mean * mean;
    let stddev = libm::sqrt(variance);
    assert!(libm::fabs(mean) < 0.3, "mean too far from 0: {mean}");
    assert!(
        libm::fabs(stddev - 1.0) < 0.3,
        "stddev too far from 1: {stddev}"
    );
}

#[test]
fn random_next_bytes_fills_array() {
    use crate::array_heap::ATYPE_BYTE;
    let mut r = RngCtx::new(11);
    let arr_idx = r.arrays.alloc(ATYPE_BYTE, 16).unwrap();
    r.call(m::nextBytes, "([B)V", &[Value::ArrayRef(arr_idx)]);
    // At least one slot should be non-zero (probability of all-zeros is 2^-128).
    let mut any_nonzero = false;
    for i in 0..16 {
        if r.arrays.load(arr_idx, i).unwrap() != 0 {
            any_nonzero = true;
            break;
        }
    }
    assert!(any_nonzero, "nextBytes left the array all zeros");
}

#[test]
fn random_next_bytes_partial_tail() {
    use crate::array_heap::ATYPE_BYTE;
    // Length not a multiple of 4 — exercises the inner-loop tail.
    let mut r = RngCtx::new(13);
    let arr_idx = r.arrays.alloc(ATYPE_BYTE, 7).unwrap();
    r.call(m::nextBytes, "([B)V", &[Value::ArrayRef(arr_idx)]);
    // Length must be unchanged (no overrun).
    assert_eq!(r.arrays.length(arr_idx), Some(7));
}
