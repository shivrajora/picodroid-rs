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

// ── HashMap native method tests ──────────────────────────────────────────

fn dispatch_map(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
    };
    BuiltinHandler
        .dispatch("java/util/HashMap", method, &mut ctx)
        .expect("HashMap method not handled")
}

fn dispatch_set(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
    };
    BuiltinHandler
        .dispatch("java/util/HashSet", method, &mut ctx)
        .expect("HashSet method not handled")
}

fn make_map(strings: &mut StringTable, objects: &mut ObjectHeap) -> Value {
    let map = Value::ObjectRef(objects.alloc("java/util/HashMap").unwrap());
    dispatch_map("<init>", "()V", &[map], strings, objects).unwrap();
    map
}

fn make_set(strings: &mut StringTable, objects: &mut ObjectHeap) -> Value {
    let set = Value::ObjectRef(objects.alloc("java/util/HashSet").unwrap());
    dispatch_set("<init>", "()V", &[set], strings, objects).unwrap();
    set
}

#[test]
fn hashmap_init_and_size() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map("size", "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_map("isEmpty", "()Z", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashmap_put_and_get() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, 10), put(2, 20), put(3, 30)
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(2), Value::Int(20)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(3), Value::Int(30)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(2)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(3)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(30)))
    );
    assert_eq!(
        dispatch_map("size", "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn hashmap_put_overwrite() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, 10) returns null (no previous)
    assert_eq!(
        dispatch_map(
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(1), Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
    // put(1, 99) returns old value 10
    assert_eq!(
        dispatch_map(
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(1), Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map("size", "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashmap_get_missing() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(42)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

#[test]
fn hashmap_remove() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            "remove",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map("size", "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_remove_missing() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map(
            "remove",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

#[test]
fn hashmap_contains_key() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(5), Value::Int(50)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            "containsKey",
            "(Ljava/lang/Object;)Z",
            &[map, Value::Int(5)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            "containsKey",
            "(Ljava/lang/Object;)Z",
            &[map, Value::Int(6)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_contains_value() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(1), Value::Int(42)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            "containsValue",
            "(Ljava/lang/Object;)Z",
            &[map, Value::Int(42)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            "containsValue",
            "(Ljava/lang/Object;)Z",
            &[map, Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_clear() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map("clear", "()V", &[map], &mut strings, &mut objects).unwrap();
    assert_eq!(
        dispatch_map("size", "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_get_or_default() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    // Key present: returns value
    assert_eq!(
        dispatch_map(
            "getOrDefault",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(1), Value::Int(-1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    // Key absent: returns default
    assert_eq!(
        dispatch_map(
            "getOrDefault",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(99), Value::Int(-1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn hashmap_integer_keys() {
    // Test with boxed Integer objects as keys (wrapper equality via field 0)
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    // Create two Integer(42) objects at different heap slots
    let int1 = objects.alloc("java/lang/Integer").unwrap();
    objects.set_field(int1, 0, Value::Int(42));
    let int2 = objects.alloc("java/lang/Integer").unwrap();
    objects.set_field(int2, 0, Value::Int(42));
    assert_ne!(int1, int2); // different heap slots

    // put with int1 as key
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::ObjectRef(int1), Value::Int(100)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // get with int2 as key — should find it via wrapper equality
    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::ObjectRef(int2)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(100)))
    );
}

#[test]
fn hashmap_string_keys() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    let key_a = Value::Reference(strings.intern(b"alpha").unwrap());
    let key_b = Value::Reference(strings.intern(b"beta").unwrap());

    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, key_a, Value::Int(1)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, key_b, Value::Int(2)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, key_a],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, key_b],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn hashmap_null_key() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Null, Value::Int(77)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Null],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(77)))
    );
}

#[test]
fn hashmap_null_value() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, null)
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(1), Value::Null],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    // containsKey should return true
    assert_eq!(
        dispatch_map(
            "containsKey",
            "(Ljava/lang/Object;)Z",
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    // get returns Null (same as "not found"), but containsKey distinguishes
    assert_eq!(
        dispatch_map(
            "get",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

// ── HashSet native method tests ──────────────────────────────────────────

#[test]
fn hashset_add_contains_remove() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    // add(10) returns true (was absent)
    assert_eq!(
        dispatch_set(
            "add",
            "(Ljava/lang/Object;)Z",
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set(
            "contains",
            "(Ljava/lang/Object;)Z",
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set("size", "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );

    // remove(10) returns true (was present)
    assert_eq!(
        dispatch_set(
            "remove",
            "(Ljava/lang/Object;)Z",
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set("size", "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashset_add_duplicate() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    dispatch_set(
        "add",
        "(Ljava/lang/Object;)Z",
        &[set, Value::Int(5)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    // Second add returns false (was already present)
    assert_eq!(
        dispatch_set(
            "add",
            "(Ljava/lang/Object;)Z",
            &[set, Value::Int(5)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_set("size", "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashset_clear() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    dispatch_set(
        "add",
        "(Ljava/lang/Object;)Z",
        &[set, Value::Int(1)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_set(
        "add",
        "(Ljava/lang/Object;)Z",
        &[set, Value::Int(2)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_set("clear", "()V", &[set], &mut strings, &mut objects).unwrap();
    assert_eq!(
        dispatch_set("size", "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_set("isEmpty", "()Z", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

// ── Regression: integer-key map then string-key map ──────────────────────

#[test]
fn hashmap_int_then_string_keys_shared_heap() {
    // Reproduces the sim bug: creating a HashMap with Integer keys, using
    // StringBuilder, then creating a second HashMap with string keys fails
    // to find the string keys.
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();

    // Map 1: Integer keys
    let m1 = Value::ObjectRef(objects.alloc("java/util/HashMap").unwrap());
    dispatch_map("<init>", "()V", &[m1], &mut strings, &mut objects).unwrap();

    // Integer.valueOf(1) — alloc Integer, set field 0
    let int1 = objects.alloc("java/lang/Integer").unwrap();
    objects.set_field(int1, 0, Value::Int(1));
    let int10 = objects.alloc("java/lang/Integer").unwrap();
    objects.set_field(int10, 0, Value::Int(10));

    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[m1, Value::ObjectRef(int1), Value::ObjectRef(int10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // Verify m1.get works
    let int1b = objects.alloc("java/lang/Integer").unwrap();
    objects.set_field(int1b, 0, Value::Int(1));
    let result = dispatch_map(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        &[m1, Value::ObjectRef(int1b)],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(result, Value::ObjectRef(int10));

    // StringBuilder usage (simulating "v1=" + v1)
    let _sb = objects.alloc("java/lang/StringBuilder").unwrap();
    objects.sb_push();
    objects.sb_append_bytes(b"v1=");
    objects.sb_append_int(10);
    let sb_bytes = objects.sb_pop();
    let _str_idx = strings.intern_dyn(&sb_bytes).unwrap();

    // Map 2: String keys
    let m2 = Value::ObjectRef(objects.alloc("java/util/HashMap").unwrap());
    dispatch_map("<init>", "()V", &[m2], &mut strings, &mut objects).unwrap();

    let hello = Value::Reference(strings.intern(b"hello").unwrap());
    let world = Value::Reference(strings.intern(b"world").unwrap());

    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[m2, hello, world],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // This should find "hello" → "world"
    let result = dispatch_map(
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        &[m2, hello],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(result, world);
}

// ── Iterator native method tests ─────────────────────────────────────────

fn dispatch_iter(
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
        .dispatch("java/util/Iterator", method, &mut ctx)
        .expect("Iterator method not handled")
}

#[test]
fn iterator_arraylist_empty() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc("java/util/ArrayList").unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();

    // Create iterator via ArrayList.iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            descriptor: "()Ljava/util/Iterator;",
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
        };
        BuiltinHandler
            .dispatch("java/util/ArrayList", "iterator", &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // hasNext should be false immediately
    assert_eq!(
        dispatch_iter("hasNext", "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_arraylist_basic() {
    let mut strings = StringTable::new();
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
    dispatch_list(
        "add",
        "(Ljava/lang/Object;)Z",
        &[list, Value::Int(30)],
        &mut objects,
    )
    .unwrap();

    // Create iterator
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            descriptor: "()Ljava/util/Iterator;",
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
        };
        BuiltinHandler
            .dispatch("java/util/ArrayList", "iterator", &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // Iterate: hasNext/next cycle
    assert_eq!(
        dispatch_iter("hasNext", "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_iter("next", "()Ljava/lang/Object;", &[iter], &mut objects),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_iter("next", "()Ljava/lang/Object;", &[iter], &mut objects),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_iter("next", "()Ljava/lang/Object;", &[iter], &mut objects),
        Ok(Some(Value::Int(30)))
    );
    assert_eq!(
        dispatch_iter("hasNext", "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_arraylist_single() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc("java/util/ArrayList").unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(
        "add",
        "(Ljava/lang/Object;)Z",
        &[list, Value::Int(42)],
        &mut objects,
    )
    .unwrap();

    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            descriptor: "()Ljava/util/Iterator;",
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
        };
        BuiltinHandler
            .dispatch("java/util/ArrayList", "iterator", &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    assert_eq!(
        dispatch_iter("hasNext", "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_iter("next", "()Ljava/lang/Object;", &[iter], &mut objects),
        Ok(Some(Value::Int(42)))
    );
    assert_eq!(
        dispatch_iter("hasNext", "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_next_past_end() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc("java/util/ArrayList").unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();

    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            descriptor: "()Ljava/util/Iterator;",
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
        };
        BuiltinHandler
            .dispatch("java/util/ArrayList", "iterator", &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // next() on empty iterator should error
    assert!(dispatch_iter("next", "()Ljava/lang/Object;", &[iter], &mut objects).is_err());
}

#[test]
fn iterator_hashmap_keys() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(2), Value::Int(20)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // keySet()
    let keyset = dispatch_map(
        "keySet",
        "()Ljava/util/Set;",
        &[map],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();

    // keySet().iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            descriptor: "()Ljava/util/Iterator;",
            args: &[keyset],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
        };
        BuiltinHandler
            .dispatch("java/util/HashMap$KeySet", "iterator", &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // Collect keys
    let mut keys = alloc::vec::Vec::new();
    while dispatch_iter("hasNext", "()Z", &[iter], &mut objects)
        .unwrap()
        .unwrap()
        == Value::Int(1)
    {
        let k = dispatch_iter("next", "()Ljava/lang/Object;", &[iter], &mut objects)
            .unwrap()
            .unwrap();
        keys.push(k);
    }
    assert_eq!(keys.len(), 2);
    // Keys should be Int(1) and Int(2) (order not guaranteed, but our impl preserves insertion order)
    assert!(keys.contains(&Value::Int(1)));
    assert!(keys.contains(&Value::Int(2)));
}

#[test]
fn iterator_hashmap_values() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[map, Value::Int(2), Value::Int(20)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // values()
    let vals = dispatch_map(
        "values",
        "()Ljava/util/Collection;",
        &[map],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();

    // values().iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            descriptor: "()Ljava/util/Iterator;",
            args: &[vals],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
        };
        BuiltinHandler
            .dispatch("java/util/HashMap$Values", "iterator", &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    let mut values = alloc::vec::Vec::new();
    while dispatch_iter("hasNext", "()Z", &[iter], &mut objects)
        .unwrap()
        .unwrap()
        == Value::Int(1)
    {
        let v = dispatch_iter("next", "()Ljava/lang/Object;", &[iter], &mut objects)
            .unwrap()
            .unwrap();
        values.push(v);
    }
    assert_eq!(values.len(), 2);
    assert!(values.contains(&Value::Int(10)));
    assert!(values.contains(&Value::Int(20)));
}

// ── Enum native method tests ─────────────────────────────────────────────

fn dispatch_enum(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
    };
    BuiltinHandler
        .dispatch("java/lang/Enum", method, &mut ctx)
        .expect("Enum method not handled")
}

fn make_enum_instance(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    name: &'static [u8],
    ordinal: i32,
) -> Value {
    let obj = Value::ObjectRef(objects.alloc("TestEnum").unwrap());
    let name_ref = Value::Reference(strings.intern(name).unwrap());
    dispatch_enum(
        "<init>",
        "(Ljava/lang/String;I)V",
        &[obj, name_ref, Value::Int(ordinal)],
        strings,
        objects,
    )
    .unwrap();
    obj
}

#[test]
fn enum_init_name_ordinal() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);

    let name = dispatch_enum(
        "name",
        "()Ljava/lang/String;",
        &[red],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let Value::Reference(idx) = name else {
        panic!("expected Reference");
    };
    assert_eq!(strings.resolve(idx), Some("RED"));

    assert_eq!(
        dispatch_enum("ordinal", "()I", &[red], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn enum_to_string() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let green = make_enum_instance(&mut objects, &mut strings, b"GREEN", 1);

    let result = dispatch_enum(
        "toString",
        "()Ljava/lang/String;",
        &[green],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let Value::Reference(idx) = result else {
        panic!("expected Reference");
    };
    assert_eq!(strings.resolve(idx), Some("GREEN"));
}

#[test]
fn enum_equals_same() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);

    assert_eq!(
        dispatch_enum(
            "equals",
            "(Ljava/lang/Object;)Z",
            &[red, red],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn enum_equals_different() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);
    let green = make_enum_instance(&mut objects, &mut strings, b"GREEN", 1);

    assert_eq!(
        dispatch_enum(
            "equals",
            "(Ljava/lang/Object;)Z",
            &[red, green],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn enum_compare_to() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);
    let blue = make_enum_instance(&mut objects, &mut strings, b"BLUE", 2);

    // RED(0).compareTo(BLUE(2)) = -2
    assert_eq!(
        dispatch_enum(
            "compareTo",
            "(Ljava/lang/Enum;)I",
            &[red, blue],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(-2)))
    );
    // BLUE(2).compareTo(RED(0)) = 2
    assert_eq!(
        dispatch_enum(
            "compareTo",
            "(Ljava/lang/Enum;)I",
            &[blue, red],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(2)))
    );
}

// ── String enhancement tests ────────────────────────────────────────────

#[test]
fn string_concat() {
    let mut ctx = StrCtx::new();
    let a = ctx.intern(b"hello");
    let b = ctx.intern(b" world");
    let result = ctx
        .dispatch("concat", "(Ljava/lang/String;)Ljava/lang/String;", &[a, b])
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
        .dispatch(
            "concat",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[a, empty],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello");
}

#[test]
fn string_hash_code_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"");
    assert_eq!(
        ctx.dispatch("hashCode", "()I", &[s]),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn string_hash_code_known() {
    // Java's "abc".hashCode() = 96354
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    assert_eq!(
        ctx.dispatch("hashCode", "()I", &[s]),
        Ok(Some(Value::Int(96354)))
    );
}

#[test]
fn string_replace_char() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hello");
    let result = ctx
        .dispatch(
            "replace",
            "(CC)Ljava/lang/String;",
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
            "replace",
            "(CC)Ljava/lang/String;",
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
            "replace",
            "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/lang/String;",
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
            "replace",
            "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/lang/String;",
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
    let result = ctx.dispatch("toCharArray", "()[C", &[s]).unwrap().unwrap();
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
    let result = ctx.dispatch("toCharArray", "()[C", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(0));
}

#[test]
fn string_split_basic() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"a,b,c");
    let delim = ctx.intern(b",");
    let result = ctx
        .dispatch(
            "split",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            &[s, delim],
        )
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
        .dispatch(
            "split",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            &[s, delim],
        )
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
        .dispatch(
            "split",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            &[s, delim],
        )
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
fn string_split_empty_parts() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"a,,b");
    let delim = ctx.intern(b",");
    let result = ctx
        .dispatch(
            "split",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            &[s, delim],
        )
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    let r1 = ((ctx.arrays.load(arr, 1).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r1), Some(""));
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
            .dispatch(
                "split",
                "(Ljava/lang/String;)[Ljava/lang/String;",
                &[s, delim],
            )
            .unwrap()
            .unwrap();
        let Value::ArrayRef(arr) = result else {
            panic!("expected ArrayRef");
        };
        assert_eq!(ctx.arrays.length(arr), Some(51));
    }
}
