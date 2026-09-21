// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── QA 2026-09-13 regression guards ──────────────────────────────────────
//
// One test per fix from the 2026-09-13/14 QA round (`docs/qa-2026-09-13.md`).
// Each asserts the Java-side answer the round found wrong, so reverting the
// fix fails the host suite instead of waiting for a nightly hardware run.

// J-fix c3561cc3: an empty target matches before every char and at the end.
#[test]
fn string_replace_empty_target_interleaves() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"ab");
    let target = ctx.intern(b"");
    let repl = ctx.intern(b"-");
    let result = ctx
        .dispatch(
            m::replace,
            d::CharSequence_CharSequence__String,
            &[s, target, repl],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "-a-b-");
}

#[test]
fn string_replace_empty_target_on_empty_receiver() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"");
    let target = ctx.intern(b"");
    let repl = ctx.intern(b"x");
    let result = ctx
        .dispatch(
            m::replace,
            d::CharSequence_CharSequence__String,
            &[s, target, repl],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "x");
}

// J-fix 44b99d9c: compareTo is the char difference, then the length
// difference -- not -1/0/1.
#[test]
fn string_compare_to_returns_char_difference() {
    let mut ctx = StrCtx::new();
    let cmp = |ctx: &mut StrCtx, a: &'static [u8], b: &'static [u8]| {
        let (a, b) = (ctx.intern(a), ctx.intern(b));
        ctx.dispatch(m::compareTo, d::String__I, &[a, b])
            .unwrap()
            .unwrap()
    };
    assert_eq!(cmp(&mut ctx, b"a", b"c"), Value::Int(-2));
    assert_eq!(cmp(&mut ctx, b"c", b"a"), Value::Int(2));
    assert_eq!(cmp(&mut ctx, b"B", b"a"), Value::Int(-31));
    assert_eq!(cmp(&mut ctx, b"abc", b"abc"), Value::Int(0));
}

#[test]
fn string_compare_to_returns_length_difference_on_a_prefix() {
    let mut ctx = StrCtx::new();
    let (a, b) = (ctx.intern(b"abc"), ctx.intern(b"abcde"));
    assert_eq!(
        ctx.dispatch(m::compareTo, d::String__I, &[a, b]),
        Ok(Some(Value::Int(-2)))
    );
    let (a, b) = (ctx.intern(b"abcde"), ctx.intern(b"abc"));
    assert_eq!(
        ctx.dispatch(m::compareTo, d::String__I, &[a, b]),
        Ok(Some(Value::Int(2)))
    );
}

// J-fix 84a9aa0e: %s of a Boolean/Character box prints what toString does.
#[test]
fn format_s_of_boolean_and_character_boxes() {
    let mut ctx = StrCtx::new();
    let t = ctx.box_primitive(c::java_lang_Boolean, Value::Int(1));
    let f = ctx.box_primitive(c::java_lang_Boolean, Value::Int(0));
    let ch = ctx.box_primitive(c::java_lang_Character, Value::Int(b'c' as i32));
    assert_eq!(ctx.fmt(b"%s %s %s", &[t, f, ch]), "true false c");
}

// J-fix 5bb0e17d: %x/%o take the box's own width -- a Byte is 8 bits.
#[test]
fn format_hex_octal_use_the_box_width() {
    let mut ctx = StrCtx::new();
    let b = ctx.box_primitive(c::java_lang_Byte, Value::Int(-1));
    assert_eq!(ctx.fmt(b"%x", &[b]), "ff");
    let s = ctx.box_primitive(c::java_lang_Short, Value::Int(-1));
    assert_eq!(ctx.fmt(b"%x", &[s]), "ffff");
    let i = ctx.box_primitive(c::java_lang_Integer, Value::Int(-1));
    assert_eq!(ctx.fmt(b"%x", &[i]), "ffffffff");
    let b = ctx.box_primitive(c::java_lang_Byte, Value::Int(-1));
    assert_eq!(ctx.fmt(b"%o", &[b]), "377");
}

// J-fix e7f46460: the floating conversions round the shortest round-trip
// digits HALF_UP, and %,f groups the integer part.
#[test]
fn format_float_rounds_half_up_on_shortest_digits() {
    let mut ctx = StrCtx::new();
    let d = |ctx: &mut StrCtx, v: f64| ctx.box_primitive(c::java_lang_Double, Value::Double(v));
    let v = d(&mut ctx, 1.005);
    assert_eq!(ctx.fmt(b"%.2f", &[v]), "1.01");
    let v = d(&mut ctx, 2.5);
    assert_eq!(ctx.fmt(b"%.0f", &[v]), "3");
    let v = d(&mut ctx, 0.35);
    assert_eq!(ctx.fmt(b"%.1f", &[v]), "0.4");
    // A carry out of the top digit lengthens the integer part.
    let v = d(&mut ctx, 9.99);
    assert_eq!(ctx.fmt(b"%.1f", &[v]), "10.0");
}

#[test]
fn format_float_grouping_flag_groups_the_integer_part() {
    let mut ctx = StrCtx::new();
    let v = ctx.box_primitive(c::java_lang_Double, Value::Double(1234567.891));
    assert_eq!(ctx.fmt(b"%,.2f", &[v]), "1,234,567.89");
}

// J-fix f08856c9 / e5d5b88c: a precision on an integral or character
// conversion throws IllegalFormatPrecisionException.
#[test]
fn format_precision_on_integral_conversions_throws() {
    for spec in [
        b"%.2d".as_slice(),
        b"%.2x".as_slice(),
        b"%.2o".as_slice(),
        b"%.2c".as_slice(),
    ] {
        let mut ctx = StrCtx::new();
        let fmt_ref = ctx.intern(match spec[3] {
            b'd' => b"%.2d",
            b'x' => b"%.2x",
            b'o' => b"%.2o",
            _ => b"%.2c",
        });
        let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(7));
        let arr = ctx.make_args(&[n]);
        let thrown = match ctx.dispatch(m::format, d::String_aObject__String, &[fmt_ref, arr]) {
            Err(JvmError::Exception(idx)) => ctx.objects.class_name(idx).unwrap(),
            other => panic!("expected a throw for {:?}, got {other:?}", spec),
        };
        assert_eq!(thrown, c::java_util_IllegalFormatPrecisionException);
    }
}

// J-fix e5d5b88c: each formatter failure throws its own
// IllegalFormatException subclass, not the family's base class.
#[test]
fn format_failures_throw_their_own_exception_class() {
    let mut ctx = StrCtx::new();
    let one = ctx.box_primitive(c::java_lang_Integer, Value::Int(1));
    assert_eq!(
        ctx.fmt_throws(b"%d %d", &[one]),
        c::java_util_MissingFormatArgumentException
    );

    let mut ctx = StrCtx::new();
    assert_eq!(
        ctx.fmt_throws(b"%q", &[]),
        c::java_util_UnknownFormatConversionException
    );

    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"not an int");
    assert_eq!(
        ctx.fmt_throws(b"%d", &[s]),
        c::java_util_IllegalFormatConversionException
    );
}

// J-fix 09fa471b: nextInt(bound) with a non-positive bound is an
// IllegalArgumentException, not a panic or a wrapped value.
#[test]
fn random_next_int_non_positive_bound_throws() {
    for bound in [0, -1, i32::MIN] {
        let mut r = RngCtx::new(1);
        let thrown = match r.try_call(m::nextInt, "(I)I", &[Value::Int(bound)]) {
            Err(JvmError::Exception(idx)) => r.objects.class_name(idx).unwrap(),
            other => panic!("expected a throw for bound {bound}, got {other:?}"),
        };
        assert_eq!(thrown, c::java_lang_IllegalArgumentException);
    }
}

// J-fix 4cc76d4e: the four parse entry points on a null string.
#[test]
fn parse_int_and_long_of_null_throw_number_format_exception() {
    for (class, method, desc) in [
        (c::java_lang_Integer, m::parseInt, d::String__I),
        (c::java_lang_Long, m::parseLong, d::String__J),
    ] {
        let mut objects = ObjectHeap::new();
        let thrown = match dispatch_boxed(class, method, desc, &[Value::Null], &mut objects) {
            Err(JvmError::Exception(idx)) => objects.class_name(idx).unwrap(),
            other => panic!("expected a throw from {method}, got {other:?}"),
        };
        assert_eq!(thrown, c::java_lang_NumberFormatException);
    }
}

#[test]
fn parse_float_and_double_of_null_throw_null_pointer_exception() {
    for (class, method, desc) in [
        (c::java_lang_Float, m::parseFloat, d::String__F),
        (c::java_lang_Double, m::parseDouble, d::String__D),
    ] {
        let mut objects = ObjectHeap::new();
        let thrown = match dispatch_boxed(class, method, desc, &[Value::Null], &mut objects) {
            Err(JvmError::Exception(idx)) => objects.class_name(idx).unwrap(),
            other => panic!("expected a throw from {method}, got {other:?}"),
        };
        assert_eq!(thrown, c::java_lang_NullPointerException);
    }
}

// J-fix 4eedea29: the xxxValue() accessors apply Java's widening and
// narrowing conversions (JLS 5.1.2/5.1.3).
#[test]
fn boxed_accessors_narrow_and_widen_as_java_does() {
    let mut objects = ObjectHeap::new();
    let boxed = |objects: &mut ObjectHeap, class: &'static str, v: Value| {
        let idx = objects.alloc(class).unwrap();
        objects.set_field(idx, 0, v);
        Value::ObjectRef(idx)
    };

    // Float.valueOf(2.5f).intValue() == 2 (was the float, unconverted).
    let f = boxed(&mut objects, c::java_lang_Float, Value::Float(2.5));
    assert_eq!(
        dispatch_boxed(c::java_lang_Float, m::intValue, "()I", &[f], &mut objects),
        Ok(Some(Value::Int(2)))
    );

    // Integer.valueOf(300).byteValue() == 44.
    let i = boxed(&mut objects, c::java_lang_Integer, Value::Int(300));
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Integer,
            m::byteValue,
            "()B",
            &[i],
            &mut objects
        ),
        Ok(Some(Value::Int(44)))
    );

    // Long.valueOf(1L << 33).intValue() == 0.
    let l = boxed(&mut objects, c::java_lang_Long, Value::Long(1i64 << 33));
    assert_eq!(
        dispatch_boxed(c::java_lang_Long, m::intValue, "()I", &[l], &mut objects),
        Ok(Some(Value::Int(0)))
    );

    // Integer.valueOf(70000).shortValue() == 4464.
    let i = boxed(&mut objects, c::java_lang_Integer, Value::Int(70000));
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Integer,
            m::shortValue,
            "()S",
            &[i],
            &mut objects
        ),
        Ok(Some(Value::Int(4464)))
    );

    // A Long widens to double, and an Integer to float.
    let l = boxed(&mut objects, c::java_lang_Long, Value::Long(7));
    assert_eq!(
        dispatch_boxed(c::java_lang_Long, m::doubleValue, "()D", &[l], &mut objects),
        Ok(Some(Value::Double(7.0)))
    );
    let i = boxed(&mut objects, c::java_lang_Integer, Value::Int(-3));
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Integer,
            m::floatValue,
            "()F",
            &[i],
            &mut objects
        ),
        Ok(Some(Value::Float(-3.0)))
    );
}

#[test]
fn boxed_int_accessor_saturates_nan_and_out_of_range_floats() {
    let mut objects = ObjectHeap::new();
    let boxed = |objects: &mut ObjectHeap, v: Value| {
        let idx = objects.alloc(c::java_lang_Double).unwrap();
        objects.set_field(idx, 0, v);
        Value::ObjectRef(idx)
    };

    let nan = boxed(&mut objects, Value::Double(f64::NAN));
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Double,
            m::intValue,
            "()I",
            &[nan],
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
    let big = boxed(&mut objects, Value::Double(1e30));
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Double,
            m::intValue,
            "()I",
            &[big],
            &mut objects
        ),
        Ok(Some(Value::Int(i32::MAX)))
    );
    let small = boxed(&mut objects, Value::Double(-1e30));
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Double,
            m::longValue,
            "()J",
            &[small],
            &mut objects
        ),
        Ok(Some(Value::Long(i64::MIN)))
    );
}

// J-fix 2fc8c535: a null array is a catchable NullPointerException, not the
// uncatchable InvalidReference that ended the app.
#[test]
fn arrays_methods_throw_null_pointer_exception_on_a_null_array() {
    for (method, desc, args) in [
        (m::sort, "([I)V", alloc::vec![Value::Null]),
        (m::fill, "([II)V", alloc::vec![Value::Null, Value::Int(0)]),
        (
            m::copyOf,
            "([II)[I",
            alloc::vec![Value::Null, Value::Int(4)],
        ),
    ] {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let thrown =
            match arrays_dispatch(method, desc, &args, &mut strings, &mut objects, &mut arrays) {
                Err(JvmError::Exception(idx)) => objects.class_name(idx).unwrap(),
                other => panic!("expected a throw from Arrays.{method}, got {other:?}"),
            };
        assert_eq!(thrown, c::java_lang_NullPointerException);
    }
}

#[test]
fn system_arraycopy_throws_null_pointer_exception_on_a_null_array() {
    let dst_null = |arrays: &mut ArrayHeap| {
        let a = make_int_array(arrays, &[1, 2, 3]);
        alloc::vec![
            Value::ArrayRef(a),
            Value::Int(0),
            Value::Null,
            Value::Int(0),
            Value::Int(3),
        ]
    };
    let src_null = |arrays: &mut ArrayHeap| {
        let a = make_int_array(arrays, &[1, 2, 3]);
        alloc::vec![
            Value::Null,
            Value::Int(0),
            Value::ArrayRef(a),
            Value::Int(0),
            Value::Int(3),
        ]
    };
    for build in [
        &dst_null as &dyn Fn(&mut ArrayHeap) -> alloc::vec::Vec<Value>,
        &src_null,
    ] {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let args = build(&mut arrays);
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: "(Ljava/lang/Object;ILjava/lang/Object;II)V",
            args: &args,
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        let r = BuiltinHandler
            .dispatch(c::java_lang_System, m::arraycopy, &mut ctx)
            .expect("System.arraycopy not handled");
        let thrown = match r {
            Err(JvmError::Exception(idx)) => objects.class_name(idx).unwrap(),
            other => panic!("expected a throw from System.arraycopy, got {other:?}"),
        };
        assert_eq!(thrown, c::java_lang_NullPointerException);
    }
}
