// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── String native method tests ────────────────────────────────────────────

#[test]
fn string_length_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_EMPTY);
    assert_eq!(
        ctx.dispatch(m::length, "()I", &[s]),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn string_length_nonempty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(
        ctx.dispatch(m::length, "()I", &[s]),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn string_char_at() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_ABC);
    assert_eq!(
        ctx.dispatch(m::charAt, "(I)C", &[s, Value::Int(0)]),
        Ok(Some(Value::Int(b'a' as i32)))
    );
    assert_eq!(
        ctx.dispatch(m::charAt, "(I)C", &[s, Value::Int(2)]),
        Ok(Some(Value::Int(b'c' as i32)))
    );
}

#[test]
fn string_index_of_string_found() {
    let mut ctx = StrCtx::new();
    let haystack = ctx.intern(S_HELLO);
    let needle = ctx.intern(S_ELL);
    assert_eq!(
        ctx.dispatch(m::indexOf, d::String__I, &[haystack, needle]),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn string_index_of_string_not_found() {
    let mut ctx = StrCtx::new();
    let haystack = ctx.intern(S_HELLO);
    let needle = ctx.intern(S_BAR);
    assert_eq!(
        ctx.dispatch(m::indexOf, d::String__I, &[haystack, needle]),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn string_index_of_char_found() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(
        ctx.dispatch(m::indexOf, "(I)I", &[s, Value::Int(b'l' as i32)]),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn string_index_of_char_not_found() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(
        ctx.dispatch(m::indexOf, "(I)I", &[s, Value::Int(b'z' as i32)]),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn string_substring() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    let result = ctx
        .dispatch(
            m::substring,
            d::I_I__String,
            &[s, Value::Int(1), Value::Int(4)],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "ell");
}

#[test]
fn string_equals() {
    let mut ctx = StrCtx::new();
    let foo1 = ctx.intern(S_FOO);
    let foo2 = ctx.intern(S_FOO);
    let bar = ctx.intern(S_BAR);
    assert_eq!(
        ctx.dispatch(m::equals, d::Object__Z, &[foo1, foo2]),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        ctx.dispatch(m::equals, d::Object__Z, &[foo1, bar]),
        Ok(Some(Value::Int(0)))
    );
    // equals(null) must return false, not an error
    assert_eq!(
        ctx.dispatch(m::equals, d::Object__Z, &[foo1, Value::Null]),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn string_starts_ends_with() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    let hel = ctx.intern(S_HEL);
    let llo = ctx.intern(S_LLO);
    assert_eq!(
        ctx.dispatch(m::startsWith, d::String__Z, &[s, hel]),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        ctx.dispatch(m::endsWith, d::String__Z, &[s, llo]),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn string_to_upper_lower() {
    let mut ctx = StrCtx::new();
    let lower = ctx.intern(S_HELLO);
    let result = ctx
        .dispatch(m::toUpperCase, d::__String, &[lower])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "HELLO");

    let upper = ctx.intern(S_UPPER_HELLO);
    let result = ctx
        .dispatch(m::toLowerCase, d::__String, &[upper])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello");
}

#[test]
fn string_trim() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_PADDED);
    let result = ctx.dispatch(m::trim, d::__String, &[s]).unwrap().unwrap();
    assert_eq!(ctx.resolve(result), "hi");
}

// ── String enhancement tests ────────────────────────────────────────────

#[test]
fn string_concat() {
    let mut ctx = StrCtx::new();
    let a = ctx.intern(b"hello");
    let b = ctx.intern(b" world");
    let result = ctx
        .dispatch(m::concat, d::String__String, &[a, b])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello world");
}

#[test]
fn string_concat_empty() {
    let mut ctx = StrCtx::new();
    let a = ctx.intern(b"hello");
    let empty = ctx.intern(b"");
    let result = ctx
        .dispatch(m::concat, d::String__String, &[a, empty])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello");
}

#[test]
fn string_hash_code_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"");
    assert_eq!(
        ctx.dispatch(m::hashCode, "()I", &[s]),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn string_hash_code_known() {
    // Java's "abc".hashCode() = 96354
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    assert_eq!(
        ctx.dispatch(m::hashCode, "()I", &[s]),
        Ok(Some(Value::Int(96354)))
    );
}

#[test]
fn string_replace_char() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hello");
    let result = ctx
        .dispatch(
            m::replace,
            d::C_C__String,
            &[s, Value::Int(b'l' as i32), Value::Int(b'r' as i32)],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "herro");
}

#[test]
fn string_replace_char_no_match() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hello");
    let result = ctx
        .dispatch(
            m::replace,
            d::C_C__String,
            &[s, Value::Int(b'z' as i32), Value::Int(b'y' as i32)],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello");
}

#[test]
fn string_replace_string() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"aXbXc");
    let target = ctx.intern(b"X");
    let repl = ctx.intern(b"YY");
    let result = ctx
        .dispatch(
            m::replace,
            d::CharSequence_CharSequence__String,
            &[s, target, repl],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "aYYbYYc");
}

#[test]
fn string_replace_string_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    let target = ctx.intern(b"b");
    let repl = ctx.intern(b"");
    let result = ctx
        .dispatch(
            m::replace,
            d::CharSequence_CharSequence__String,
            &[s, target, repl],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "ac");
}

#[test]
fn string_to_char_array() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    let result = ctx.dispatch(m::toCharArray, "()[C", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    assert_eq!(ctx.arrays.load(arr, 0), Some(b'a' as i32));
    assert_eq!(ctx.arrays.load(arr, 1), Some(b'b' as i32));
    assert_eq!(ctx.arrays.load(arr, 2), Some(b'c' as i32));
}

#[test]
fn string_to_char_array_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"");
    let result = ctx.dispatch(m::toCharArray, "()[C", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(0));
}

#[test]
fn string_get_bytes() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    let result = ctx.dispatch(m::getBytes, "()[B", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.atype(arr), Some(crate::array_heap::ATYPE_BYTE));
    assert_eq!(ctx.arrays.length(arr), Some(3));
    assert_eq!(ctx.arrays.load(arr, 0), Some(b'a' as i32));
    assert_eq!(ctx.arrays.load(arr, 1), Some(b'b' as i32));
    assert_eq!(ctx.arrays.load(arr, 2), Some(b'c' as i32));
}

#[test]
fn string_get_bytes_sign_extends() {
    let mut ctx = StrCtx::new();
    // 0xC2 0xB0 = UTF-8 "°" — high bytes must come back as negative i32s,
    // matching baload's sign-extension of byte[] slots.
    let s = ctx.intern(b"\xC2\xB0");
    let result = ctx.dispatch(m::getBytes, "()[B", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.load(arr, 0), Some(0xC2u8 as i8 as i32));
    assert_eq!(ctx.arrays.load(arr, 1), Some(0xB0u8 as i8 as i32));
}

/// `new String(byte[])` native arm: returns the interned Reference (the
/// interpreter's `finalize_invoke` swaps it for the placeholder receiver).
#[test]
fn string_init_from_bytes() {
    let mut ctx = StrCtx::new();
    let arr = ctx.arrays.alloc(crate::array_heap::ATYPE_BYTE, 3).unwrap();
    for (i, b) in [b'h', b'e', b'y'].iter().enumerate() {
        ctx.arrays.store(arr, i, *b as i32);
    }
    let result = ctx
        .dispatch("<init>", "([B)V", &[Value::Null, Value::ArrayRef(arr)])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hey");
}

#[test]
fn string_init_from_bytes_range_and_bounds() {
    let mut ctx = StrCtx::new();
    let arr = ctx.arrays.alloc(crate::array_heap::ATYPE_BYTE, 5).unwrap();
    for (i, b) in b"abcde".iter().enumerate() {
        ctx.arrays.store(arr, i, *b as i32);
    }
    let result = ctx
        .dispatch(
            "<init>",
            "([BII)V",
            &[
                Value::Null,
                Value::ArrayRef(arr),
                Value::Int(1),
                Value::Int(3),
            ],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "bcd");

    // off+len past the end / negative len → bounds error, not a panic.
    for (off, len) in [(3, 3), (0, 6), (-1, 2), (0, -1)] {
        let r = ctx.dispatch(
            "<init>",
            "([BII)V",
            &[
                Value::Null,
                Value::ArrayRef(arr),
                Value::Int(off),
                Value::Int(len),
            ],
        );
        assert_eq!(
            r,
            Err(JvmError::ArrayIndexOutOfBounds),
            "off={off} len={len}"
        );
    }
}

#[test]
fn string_init_from_bytes_sanitizes_non_ascii() {
    let mut ctx = StrCtx::new();
    let arr = ctx.arrays.alloc(crate::array_heap::ATYPE_BYTE, 3).unwrap();
    ctx.arrays.store(arr, 0, b'a' as i32);
    ctx.arrays.store(arr, 1, 0xC2u8 as i8 as i32); // high byte → '?'
    ctx.arrays.store(arr, 2, b'z' as i32);
    let result = ctx
        .dispatch("<init>", "([B)V", &[Value::Null, Value::ArrayRef(arr)])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "a?z");
}

#[test]
fn string_split_basic() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"a,b,c");
    let delim = ctx.intern(b",");
    let result = ctx
        .dispatch(m::split, d::String__aString, &[s, delim])
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    let r0 = ((ctx.arrays.load(arr, 0).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    let r1 = ((ctx.arrays.load(arr, 1).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    let r2 = ((ctx.arrays.load(arr, 2).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r0), Some("a"));
    assert_eq!(ctx.strings.resolve(r1), Some("b"));
    assert_eq!(ctx.strings.resolve(r2), Some("c"));
}

#[test]
fn string_split_no_match() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hello");
    let delim = ctx.intern(b",");
    let result = ctx
        .dispatch(m::split, d::String__aString, &[s, delim])
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(1));
    let r0 = ((ctx.arrays.load(arr, 0).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r0), Some("hello"));
}

#[test]
fn string_split_multi_char() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"a::b::c");
    let delim = ctx.intern(b"::");
    let result = ctx
        .dispatch(m::split, d::String__aString, &[s, delim])
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    let r0 = ((ctx.arrays.load(arr, 0).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    let r1 = ((ctx.arrays.load(arr, 1).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    let r2 = ((ctx.arrays.load(arr, 2).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r0), Some("a"));
    assert_eq!(ctx.strings.resolve(r1), Some("b"));
    assert_eq!(ctx.strings.resolve(r2), Some("c"));
}

#[test]
fn string_equals_non_string_is_false() {
    // "x".equals(someObject) / equals(array) is specified to be false —
    // it was a hard InvalidReference error (uncatchable).
    let mut ctx = StrCtx::new();
    let s = ctx.intern(m::x.as_bytes());
    let obj = Value::ObjectRef(ctx.objects.alloc("Foo").unwrap());
    let arr = Value::ArrayRef(ctx.arrays.alloc(crate::array_heap::ATYPE_INT, 1).unwrap());
    for other in [obj, arr, Value::Null] {
        assert_eq!(
            ctx.dispatch(m::equals, d::Object__Z, &[s, other]),
            Ok(Some(Value::Int(0))),
            "equals({other:?})"
        );
    }
    let same = ctx.intern(m::x.as_bytes());
    assert_eq!(
        ctx.dispatch(m::equals, d::Object__Z, &[s, same]),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn string_char_at_out_of_range_throws() {
    // Java: StringIndexOutOfBoundsException, not '\0'.
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    for i in [3, -1, 100] {
        let r = ctx.dispatch(m::charAt, "(I)C", &[s, Value::Int(i)]);
        let Err(JvmError::Exception(idx)) = r else {
            panic!("charAt({i}) = {r:?}");
        };
        assert_eq!(
            ctx.objects.class_name(idx),
            Some(c::java_lang_StringIndexOutOfBoundsException)
        );
    }
    assert_eq!(
        ctx.dispatch(m::charAt, "(I)C", &[s, Value::Int(2)]),
        Ok(Some(Value::Int(b'c' as i32)))
    );
}

#[test]
fn string_index_of_honours_from_index() {
    // The 2-arg overloads used to drop fromIndex entirely, so the classic
    // `while ((i = s.indexOf(x, i + 1)) >= 0)` loop never terminated.
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abcabc");
    let a = ctx.intern(b"a");
    let abc = ctx.intern(b"abc");
    let d = |ctx: &mut StrCtx, m: &str, desc: &str, args: &[Value]| -> i32 {
        match ctx.dispatch(m, desc, args) {
            Ok(Some(Value::Int(i))) => i,
            other => panic!("{m}{desc} -> {other:?}"),
        }
    };
    let so = d::String_I__I;
    let co = "(II)I";
    assert_eq!(d(&mut ctx, m::indexOf, so, &[s, a, Value::Int(1)]), 3);
    assert_eq!(d(&mut ctx, m::indexOf, so, &[s, a, Value::Int(4)]), -1);
    assert_eq!(d(&mut ctx, m::indexOf, so, &[s, a, Value::Int(-5)]), 0);
    assert_eq!(d(&mut ctx, m::indexOf, so, &[s, a, Value::Int(99)]), -1);
    assert_eq!(
        d(
            &mut ctx,
            m::indexOf,
            co,
            &[s, Value::Int(b'c' as i32), Value::Int(3)]
        ),
        5
    );
    assert_eq!(d(&mut ctx, m::lastIndexOf, so, &[s, abc, Value::Int(3)]), 3);
    assert_eq!(d(&mut ctx, m::lastIndexOf, so, &[s, abc, Value::Int(2)]), 0);
    assert_eq!(d(&mut ctx, m::lastIndexOf, so, &[s, a, Value::Int(-1)]), -1);
    assert_eq!(
        d(
            &mut ctx,
            m::lastIndexOf,
            co,
            &[s, Value::Int(b'a' as i32), Value::Int(2)]
        ),
        0
    );
    assert_eq!(
        d(
            &mut ctx,
            m::lastIndexOf,
            co,
            &[s, Value::Int(b'a' as i32), Value::Int(99)]
        ),
        3
    );
    // startsWith(prefix, toffset)
    let bc = ctx.intern(b"bc");
    let sw = d::String_I__Z;
    assert_eq!(d(&mut ctx, m::startsWith, sw, &[s, bc, Value::Int(1)]), 1);
    assert_eq!(d(&mut ctx, m::startsWith, sw, &[s, bc, Value::Int(0)]), 0);
    assert_eq!(d(&mut ctx, m::startsWith, sw, &[s, bc, Value::Int(-1)]), 0);
    assert_eq!(d(&mut ctx, m::startsWith, sw, &[s, bc, Value::Int(6)]), 0);
}

#[test]
fn string_value_of_char_newline_passes_through() {
    // Same defect StringBuilder.append(char) had (5d5f0a6): `.max(0x20)`
    // turned '\n'/'\t' into spaces, so String.valueOf('\n') joined lines
    // with a space.
    let mut ctx = StrCtx::new();
    for (c, want) in [
        (b'\n', "\n"),
        (b'\t', "\t"),
        (b'\r', "\r"),
        (b'a', "a"),
        (0x07u8, " "),
    ] {
        let r = ctx
            .dispatch(m::valueOf, d::C__String, &[Value::Int(c as i32)])
            .unwrap()
            .unwrap();
        assert_eq!(ctx.resolve(r), want, "valueOf({c:#x})");
    }
}

#[test]
fn string_split_empty_parts() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"a,,b");
    let delim = ctx.intern(b",");
    let result = ctx
        .dispatch(m::split, d::String__aString, &[s, delim])
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    let r1 = ((ctx.arrays.load(arr, 1).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r1), Some(""));
}

#[test]
fn string_split_drops_trailing_empty_strings() {
    // Java's split(regex) has limit 0: trailing empty strings are removed,
    // interior ones kept, and a no-match input yields [input].
    fn split_len(ctx: &mut StrCtx, s: &'static [u8], d: &'static [u8]) -> u16 {
        let s = ctx.intern(s);
        let d = ctx.intern(d);
        let r = ctx
            .dispatch(m::split, d::String__aString, &[s, d])
            .unwrap()
            .unwrap();
        let Value::ArrayRef(arr) = r else {
            panic!("expected ArrayRef");
        };
        ctx.arrays.length(arr).unwrap()
    }
    let mut ctx = StrCtx::new();
    assert_eq!(split_len(&mut ctx, b"a,b,,", b","), 2);
    assert_eq!(split_len(&mut ctx, b",,", b","), 0);
    assert_eq!(split_len(&mut ctx, b"a,,b", b","), 3);
    assert_eq!(split_len(&mut ctx, b",a", b","), 2);
    assert_eq!(split_len(&mut ctx, b"", b","), 1);
    assert_eq!(split_len(&mut ctx, b"abc", b","), 1);
}

// ── Stress: split many times with GC pressure ─────────────────────────────

#[test]
fn string_split_stress() {
    // Split a 200-char string with 50 delimiters (51 parts). Repeat many times
    // and verify each iteration produces the expected parts.
    let mut ctx = StrCtx::new();
    static BIG: &[u8] = b"0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50";
    let s = ctx.intern(BIG);
    let delim = ctx.intern(b",");
    for _ in 0..20 {
        let result = ctx
            .dispatch(m::split, d::String__aString, &[s, delim])
            .unwrap()
            .unwrap();
        let Value::ArrayRef(arr) = result else {
            panic!("expected ArrayRef");
        };
        assert_eq!(ctx.arrays.length(arr), Some(51));
    }
}

// ── String.join ───────────────────────────────────────────────────────────

#[test]
fn string_join_varargs_basic() {
    let mut ctx = StrCtx::new();
    assert_eq!(
        join_array(&mut ctx, b", ", &[Some(b"a"), Some(b"b"), Some(b"c")]),
        "a, b, c"
    );
}

#[test]
fn string_join_varargs_empty_and_single() {
    let mut ctx = StrCtx::new();
    assert_eq!(join_array(&mut ctx, b"-", &[]), "");
    assert_eq!(join_array(&mut ctx, b"-", &[Some(b"only")]), "only");
    assert_eq!(join_array(&mut ctx, b"", &[Some(b"x"), Some(b"y")]), "xy");
}

#[test]
fn string_join_varargs_null_element_prints_null() {
    let mut ctx = StrCtx::new();
    assert_eq!(
        join_array(&mut ctx, b"/", &[Some(b"a"), None, Some(b"c")]),
        "a/null/c"
    );
}

#[test]
fn string_join_rejects_non_string_element() {
    let mut ctx = StrCtx::new();
    let d = ctx.intern(b",");
    let arr = ctx.arrays.alloc(crate::array_heap::ATYPE_REF, 1).unwrap();
    let inner = ctx.arrays.alloc(crate::array_heap::ATYPE_REF, 0).unwrap();
    ctx.arrays.store(
        arr,
        0,
        ((inner as u32) | crate::array_heap::ARRAY_TAG) as i32,
    );
    let r = ctx.dispatch(
        m::join,
        d::CharSequence_aCharSequence__String,
        &[d, Value::ArrayRef(arr)],
    );
    assert!(matches!(r, Err(JvmError::InvalidReference)));
}

#[test]
fn string_join_unknown_descriptor_is_not_served() {
    let mut ctx = StrCtx::new();
    let d = ctx.intern(b",");
    let mut nctx = NativeContext {
        classes: &[],
        descriptor: "(Ljava/lang/String;)Ljava/lang/String;",
        args: &[d],
        strings: &mut ctx.strings,
        objects: &mut ctx.objects,
        arrays: &mut ctx.arrays,
        upcall: None,
    };
    assert!(BuiltinHandler
        .dispatch(c::java_lang_String, m::join, &mut nctx)
        .is_none());
}
