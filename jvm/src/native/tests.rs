use super::*;
use crate::{array_heap::ArrayHeap, heap::StringTable, object_heap::ObjectHeap};

// ── String helper ─────────────────────────────────────────────────────────
//
// Holds the per-test state (strings, objects, arrays) so callers can intern
// strings before dispatching and resolve returned string references afterward.
struct StrCtx {
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
}

impl StrCtx {
    fn new() -> Self {
        Self {
            strings: StringTable::new(),
            objects: ObjectHeap::new(),
            arrays: ArrayHeap::new(),
        }
    }

    /// Intern a static byte slice and return it as a Value::Reference.
    fn intern(&mut self, s: &'static [u8]) -> Value {
        Value::Reference(self.strings.intern(s).unwrap())
    }

    /// Dispatch a String method with the given args.
    fn dispatch(
        &mut self,
        method: &str,
        desc: &str,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        let mut ctx = NativeContext {
            descriptor: desc,
            args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
        };
        BuiltinHandler
            .dispatch("java/lang/String", method, &mut ctx)
            .expect("String method not handled")
    }

    /// Resolve a Value::Reference to a &str (for asserting string output).
    fn resolve(&self, v: Value) -> &str {
        if let Value::Reference(idx) = v {
            self.strings.resolve(idx).unwrap_or("")
        } else {
            panic!("expected Reference, got {v:?}")
        }
    }
}

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

// ── String native method tests ────────────────────────────────────────────

static S_EMPTY: &[u8] = b"";
static S_HELLO: &[u8] = b"hello";
static S_ABC: &[u8] = b"abc";
static S_FOO: &[u8] = b"foo";
static S_BAR: &[u8] = b"bar";
static S_ELL: &[u8] = b"ell";
static S_HEL: &[u8] = b"hel";
static S_LLO: &[u8] = b"llo";
static S_PADDED: &[u8] = b"  hi  ";
static S_UPPER_HELLO: &[u8] = b"HELLO";

#[test]
fn string_length_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_EMPTY);
    assert_eq!(ctx.dispatch("length", "()I", &[s]), Ok(Some(Value::Int(0))));
}

#[test]
fn string_length_nonempty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(ctx.dispatch("length", "()I", &[s]), Ok(Some(Value::Int(5))));
}

#[test]
fn string_char_at() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_ABC);
    assert_eq!(
        ctx.dispatch("charAt", "(I)C", &[s, Value::Int(0)]),
        Ok(Some(Value::Int(b'a' as i32)))
    );
    assert_eq!(
        ctx.dispatch("charAt", "(I)C", &[s, Value::Int(2)]),
        Ok(Some(Value::Int(b'c' as i32)))
    );
}

#[test]
fn string_index_of_string_found() {
    let mut ctx = StrCtx::new();
    let haystack = ctx.intern(S_HELLO);
    let needle = ctx.intern(S_ELL);
    assert_eq!(
        ctx.dispatch("indexOf", "(Ljava/lang/String;)I", &[haystack, needle]),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn string_index_of_string_not_found() {
    let mut ctx = StrCtx::new();
    let haystack = ctx.intern(S_HELLO);
    let needle = ctx.intern(S_BAR);
    assert_eq!(
        ctx.dispatch("indexOf", "(Ljava/lang/String;)I", &[haystack, needle]),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn string_index_of_char_found() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(
        ctx.dispatch("indexOf", "(I)I", &[s, Value::Int(b'l' as i32)]),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn string_index_of_char_not_found() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(
        ctx.dispatch("indexOf", "(I)I", &[s, Value::Int(b'z' as i32)]),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn string_substring() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    let result = ctx
        .dispatch(
            "substring",
            "(II)Ljava/lang/String;",
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
        ctx.dispatch("equals", "(Ljava/lang/Object;)Z", &[foo1, foo2]),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        ctx.dispatch("equals", "(Ljava/lang/Object;)Z", &[foo1, bar]),
        Ok(Some(Value::Int(0)))
    );
    // equals(null) must return false, not an error
    assert_eq!(
        ctx.dispatch("equals", "(Ljava/lang/Object;)Z", &[foo1, Value::Null]),
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
        ctx.dispatch("startsWith", "(Ljava/lang/String;)Z", &[s, hel]),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        ctx.dispatch("endsWith", "(Ljava/lang/String;)Z", &[s, llo]),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn string_to_upper_lower() {
    let mut ctx = StrCtx::new();
    let lower = ctx.intern(S_HELLO);
    let result = ctx
        .dispatch("toUpperCase", "()Ljava/lang/String;", &[lower])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "HELLO");

    let upper = ctx.intern(S_UPPER_HELLO);
    let result = ctx
        .dispatch("toLowerCase", "()Ljava/lang/String;", &[upper])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello");
}

#[test]
fn string_trim() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_PADDED);
    let result = ctx
        .dispatch("trim", "()Ljava/lang/String;", &[s])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hi");
}

// ── StringBuilder native method tests ─────────────────────────────────────
//
// StringBuilder uses a singleton buffer inside ObjectHeap, so all dispatch
// calls in a single test share the same ObjectHeap instance.

struct SbCtx {
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
}

impl SbCtx {
    fn new() -> Self {
        Self {
            strings: StringTable::new(),
            objects: ObjectHeap::new(),
            arrays: ArrayHeap::new(),
        }
    }

    fn call(
        &mut self,
        method: &str,
        desc: &str,
        extra: Option<Value>,
    ) -> Result<Option<Value>, JvmError> {
        // args[0] is the fake `this` — never dereferenced by sb methods
        let this = Value::ObjectRef(0);
        let args: alloc::vec::Vec<Value> = match extra {
            None => alloc::vec![this],
            Some(v) => alloc::vec![this, v],
        };
        let mut ctx = NativeContext {
            descriptor: desc,
            args: &args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
        };
        BuiltinHandler
            .dispatch("java/lang/StringBuilder", method, &mut ctx)
            .expect("StringBuilder method not handled")
    }

    fn to_string(&mut self) -> &str {
        let result = self
            .call("toString", "()Ljava/lang/String;", None)
            .unwrap()
            .unwrap();
        if let Value::Reference(idx) = result {
            // SAFETY: the string is interned into self.strings and lives as long as self
            let ptr = self.strings.resolve(idx).unwrap_or("") as *const str;
            unsafe { &*ptr }
        } else {
            panic!("toString returned non-Reference")
        }
    }
}

#[test]
fn sb_init_empty_to_string() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    assert_eq!(ctx.to_string(), "");
}

#[test]
fn sb_append_string() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let s = ctx.strings.intern(b"hello").unwrap();
    ctx.call(
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
        Some(Value::Reference(s)),
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "hello");
}

#[test]
fn sb_append_int() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(
        "append",
        "(I)Ljava/lang/StringBuilder;",
        Some(Value::Int(42)),
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "42");
}

#[test]
fn sb_append_char() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(
        "append",
        "(C)Ljava/lang/StringBuilder;",
        Some(Value::Int(b'A' as i32)),
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "A");
}

#[test]
fn sb_append_bool_true() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(
        "append",
        "(Z)Ljava/lang/StringBuilder;",
        Some(Value::Int(1)),
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "true");
}

#[test]
fn sb_append_bool_false() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(
        "append",
        "(Z)Ljava/lang/StringBuilder;",
        Some(Value::Int(0)),
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "false");
}

#[test]
fn sb_length_and_char_at() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let s = ctx.strings.intern(b"abc").unwrap();
    ctx.call(
        "append",
        "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
        Some(Value::Reference(s)),
    )
    .unwrap();
    assert_eq!(ctx.call("length", "()I", None), Ok(Some(Value::Int(3))));
    assert_eq!(
        ctx.call("charAt", "(I)C", Some(Value::Int(1))),
        Ok(Some(Value::Int(b'b' as i32)))
    );
}

// ── Boxed type tests ──────────────────────────────────────────────────────
//
// Each test allocates a boxed object via valueOf, then reads it back via
// the unboxing accessor, sharing the same ObjectHeap across both calls.

fn dispatch_boxed(
    class: &str,
    method: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        descriptor: desc,
        args,
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
    };
    BuiltinHandler
        .dispatch(class, method, &mut ctx)
        .expect("boxed method not handled")
}

#[test]
fn integer_value_of_and_int_value() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        "java/lang/Integer",
        "valueOf",
        "(I)Ljava/lang/Integer;",
        &[Value::Int(42)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            "java/lang/Integer",
            "intValue",
            "()I",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Int(42)))
    );
}

#[test]
fn boolean_value_of_true() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        "java/lang/Boolean",
        "valueOf",
        "(Z)Ljava/lang/Boolean;",
        &[Value::Int(1)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            "java/lang/Boolean",
            "booleanValue",
            "()Z",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn long_value_of_and_long_value() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        "java/lang/Long",
        "valueOf",
        "(J)Ljava/lang/Long;",
        &[Value::Long(1000)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed("java/lang/Long", "longValue", "()J", &[boxed], &mut objects),
        Ok(Some(Value::Long(1000)))
    );
}

#[test]
fn float_value_of_and_float_value() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        "java/lang/Float",
        "valueOf",
        "(F)Ljava/lang/Float;",
        &[Value::Float(3.14)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            "java/lang/Float",
            "floatValue",
            "()F",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Float(3.14)))
    );
}

#[test]
fn double_value_of_and_double_value() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        "java/lang/Double",
        "valueOf",
        "(D)Ljava/lang/Double;",
        &[Value::Double(2.71)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            "java/lang/Double",
            "doubleValue",
            "()D",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Double(2.71)))
    );
}

// ── ArrayList / Collections tests ─────────────────────────────────────────

fn dispatch_list(
    method: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        descriptor: desc,
        args,
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
    };
    BuiltinHandler
        .dispatch("java/util/ArrayList", method, &mut ctx)
        .expect("ArrayList method not handled")
}

#[test]
fn arraylist_init_and_size() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc("java/util/ArrayList").unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    assert_eq!(
        dispatch_list("size", "()I", &[list], &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_list("isEmpty", "()Z", &[list], &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn arraylist_add_and_get() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc("java/util/ArrayList").unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(
        "add",
        "(Ljava/lang/Object;)Z",
        &[list, Value::Int(10)],
        &mut objects,
    )
    .unwrap();
    dispatch_list(
        "add",
        "(Ljava/lang/Object;)Z",
        &[list, Value::Int(20)],
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_list(
            "get",
            "(I)Ljava/lang/Object;",
            &[list, Value::Int(0)],
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_list(
            "get",
            "(I)Ljava/lang/Object;",
            &[list, Value::Int(1)],
            &mut objects
        ),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_list("size", "()I", &[list], &mut objects),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn arraylist_set_returns_old() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc("java/util/ArrayList").unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(
        "add",
        "(Ljava/lang/Object;)Z",
        &[list, Value::Int(1)],
        &mut objects,
    )
    .unwrap();
    // set(0, 99) returns the old value Int(1)
    assert_eq!(
        dispatch_list(
            "set",
            "(ILjava/lang/Object;)Ljava/lang/Object;",
            &[list, Value::Int(0), Value::Int(99)],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_list(
            "get",
            "(I)Ljava/lang/Object;",
            &[list, Value::Int(0)],
            &mut objects
        ),
        Ok(Some(Value::Int(99)))
    );
}

#[test]
fn arraylist_remove() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc("java/util/ArrayList").unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(
        "add",
        "(Ljava/lang/Object;)Z",
        &[list, Value::Int(5)],
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_list(
            "remove",
            "(I)Ljava/lang/Object;",
            &[list, Value::Int(0)],
            &mut objects
        ),
        Ok(Some(Value::Int(5)))
    );
    assert_eq!(
        dispatch_list("size", "()I", &[list], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn arraylist_contains() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc("java/util/ArrayList").unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(
        "add",
        "(Ljava/lang/Object;)Z",
        &[list, Value::Int(7)],
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_list(
            "contains",
            "(Ljava/lang/Object;)Z",
            &[list, Value::Int(7)],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_list(
            "contains",
            "(Ljava/lang/Object;)Z",
            &[list, Value::Int(8)],
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}
