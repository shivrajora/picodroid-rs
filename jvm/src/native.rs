use crate::{
    array_heap::ArrayHeap,
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

/// Context passed to [`NativeMethodHandler::dispatch`] for every native call.
///
/// All JVM heap state needed to implement a native method is accessible through
/// this struct, avoiding a large parameter list on the trait method.
pub struct NativeContext<'a> {
    /// JVM method descriptor of the called method, e.g. `"(ILjava/lang/String;)V"`.
    pub descriptor: &'a str,
    /// Method arguments.  For instance methods, `args[0]` is the receiver
    /// (`this`) as a [`Value::ObjectRef`].
    pub args: &'a [Value],
    /// Interned string storage.  Use [`StringTable::resolve`] to turn a
    /// [`Value::Reference`] index into a `&str`.
    pub strings: &'a mut StringTable,
    /// Object instance storage.
    pub objects: &'a mut ObjectHeap,
    /// Array storage.
    pub arrays: &'a mut ArrayHeap,
}

/// Callback interface for resolving Java `native` methods at runtime.
///
/// Implement this trait to connect the JVM to your platform.  The interpreter
/// calls [`dispatch`](NativeMethodHandler::dispatch) whenever it encounters a
/// native method or a method that is not found in any loaded `.class` file.
///
/// # Return convention
///
/// | Return value | Meaning |
/// |---|---|
/// | `Some(Ok(Some(v)))` | Method returned value `v` |
/// | `Some(Ok(None))` | Method returned `void` (or a value the caller ignores) |
/// | `Some(Err(e))` | Method faulted with error `e` |
/// | `None` | This handler does not recognise the call; try [`BuiltinHandler`] next |
///
/// # Example
///
/// ```rust,ignore
/// use pico_jvm::{NativeContext, NativeMethodHandler};
/// use pico_jvm::types::{JvmError, Value};
///
/// struct MyHandler;
///
/// impl NativeMethodHandler for MyHandler {
///     fn dispatch(
///         &mut self,
///         class_name: &str,
///         method_name: &str,
///         ctx: &mut NativeContext<'_>,
///     ) -> Option<Result<Option<Value>, JvmError>> {
///         match (class_name, method_name) {
///             ("com/example/Io", "println") => {
///                 if let Some(Value::Reference(idx)) = ctx.args.first() {
///                     let s = ctx.strings.resolve(*idx).unwrap_or("");
///                     // write `s` to your output
///                 }
///                 Some(Ok(None))
///             }
///             _ => None,
///         }
///     }
/// }
/// ```
pub trait NativeMethodHandler {
    /// Attempt to handle a native method call.
    ///
    /// Return `None` to indicate that this handler does not recognise the call.
    /// The interpreter will then try [`BuiltinHandler`], and finally return
    /// [`JvmError::NoSuchMethod`] if neither handler claims the call.
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>>;

    /// Returns `true` if the JVM should stop at the next opcode boundary.
    ///
    /// The interpreter checks this once per bytecode instruction.  When `true`,
    /// execution is aborted by returning [`JvmError::Interrupted`] — a clean,
    /// cooperative exit for use cases like hot-swap app deployment.
    ///
    /// Default implementation always returns `false` (never interrupted).
    fn interrupted(&self) -> bool {
        false
    }
}

/// Built-in handler for `java/lang/*` methods common to all JVM environments.
///
/// The interpreter tries the user-supplied [`NativeMethodHandler`] first, then
/// falls back to this handler automatically — you do not need to call it
/// directly or forward to it.
///
/// # Handled methods
///
/// | Class | Methods |
/// |---|---|
/// | `java/lang/Object` | `<init>` |
/// | `java/lang/Throwable` | `<init>`, `addSuppressed` |
/// | `java/lang/Exception` | `<init>` |
/// | `java/lang/RuntimeException` | `<init>` |
/// | `java/lang/StringBuilder` | `<init>`, `<init>(String)`, `append(String/int/char/long/float/double/boolean)`, `length`, `charAt`, `toString` |
/// | `java/lang/String` | `length`, `charAt`, `equals`, `equalsIgnoreCase`, `startsWith`, `endsWith`, `contains`, `indexOf`, `lastIndexOf`, `isEmpty`, `compareTo`, `substring`, `trim`, `toUpperCase`, `toLowerCase`, `valueOf` |
/// | `java/lang/Integer` | `<init>`, `valueOf`, `intValue` |
/// | `java/lang/Boolean` | `<init>`, `valueOf`, `booleanValue` |
/// | `java/lang/Long` | `<init>`, `valueOf`, `longValue` |
/// | `java/lang/Float` | `<init>`, `valueOf`, `floatValue` |
/// | `java/lang/Double` | `<init>`, `valueOf`, `doubleValue` |
/// | `java/util/ArrayList` | `<init>`, `add`, `get`, `size`, `isEmpty`, `set`, `remove`, `clear`, `contains` |
/// | `java/lang/Math` | `abs`, `min`, `max`, `sqrt`, `pow`, `floor`, `ceil`, `round`, `sin`, `cos`, `tan`, `atan2`, `toRadians`, `toDegrees`, `log`, `log10`, `exp` |
pub struct BuiltinHandler;

/// Extract the list buffer index stored in field 0 of an ArrayList receiver.
fn get_list_buf(objects: &ObjectHeap, args: &[Value]) -> Result<u16, JvmError> {
    let Value::ObjectRef(obj_idx) = args.first().copied().unwrap_or(Value::Null) else {
        return Err(JvmError::InvalidReference);
    };
    match objects.get_field(obj_idx, 0) {
        Some(Value::Int(n)) => Ok(n as u16),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Value equality for ArrayList.contains — uses value-based equality for
/// autoboxed wrapper objects so that `contains(42)` finds `Integer(42)` even
/// when the two `ObjectRef` indices differ (i.e., different heap slots).
fn values_eq(a: Value, b: Value, objects: &ObjectHeap) -> bool {
    match (a, b) {
        (Value::ObjectRef(ai), Value::ObjectRef(bi)) if ai != bi => {
            // Compare field 0 for wrapper equality (Integer, Long, Boolean, etc.)
            let fa = objects.get_field(ai, 0);
            fa.is_some() && fa == objects.get_field(bi, 0)
        }
        _ => a == b,
    }
}

impl BuiltinHandler {
    fn dispatch_string(
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match method_name {
            // ── String — non-allocating ──────────────────────────────────
            "length" => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    Some(Ok(Some(Value::Int(s.len() as i32))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            "charAt" => {
                if let (Some(Value::Reference(idx)), Some(Value::Int(i))) =
                    (ctx.args.first(), ctx.args.get(1))
                {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    let ch = s.as_bytes().get(*i as usize).copied().unwrap_or(0);
                    Some(Ok(Some(Value::Int(ch as i32))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            "isEmpty" => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    Some(Ok(Some(Value::Int(s.is_empty() as i32))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            "equals" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int((sa == sb) as i32))))
                }
                (Some(Value::Reference(_)), Some(Value::Null)) => Some(Ok(Some(Value::Int(0)))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "equalsIgnoreCase" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int(sa.eq_ignore_ascii_case(sb) as i32))))
                }
                (Some(Value::Reference(_)), Some(Value::Null)) => Some(Ok(Some(Value::Int(0)))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "startsWith" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int(sa.starts_with(sb) as i32))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "endsWith" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int(sa.ends_with(sb) as i32))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "contains" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int(sa.contains(sb) as i32))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "indexOf" => {
                match (ctx.args.first(), ctx.args.get(1)) {
                    (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                        // indexOf(String)
                        let sa = ctx.strings.resolve(*a).unwrap_or("");
                        let sb = ctx.strings.resolve(*b).unwrap_or("");
                        let result = sa.find(sb).map(|i| i as i32).unwrap_or(-1);
                        Some(Ok(Some(Value::Int(result))))
                    }
                    (Some(Value::Reference(a)), Some(Value::Int(ch))) => {
                        // indexOf(char) — char passed as int
                        let sa = ctx.strings.resolve(*a).unwrap_or("");
                        let target = *ch as u8;
                        let result = sa
                            .as_bytes()
                            .iter()
                            .position(|&b| b == target)
                            .map(|i| i as i32)
                            .unwrap_or(-1);
                        Some(Ok(Some(Value::Int(result))))
                    }
                    _ => Some(Err(JvmError::InvalidReference)),
                }
            }
            "lastIndexOf" => {
                match (ctx.args.first(), ctx.args.get(1)) {
                    (Some(Value::Reference(a)), Some(Value::Int(ch))) => {
                        let sa = ctx.strings.resolve(*a).unwrap_or("");
                        let target = *ch as u8;
                        let result = sa
                            .as_bytes()
                            .iter()
                            .rposition(|&b| b == target)
                            .map(|i| i as i32)
                            .unwrap_or(-1);
                        Some(Ok(Some(Value::Int(result))))
                    }
                    (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                        // lastIndexOf(String)
                        let sa = ctx.strings.resolve(*a).unwrap_or("");
                        let sb = ctx.strings.resolve(*b).unwrap_or("");
                        let result = sa.rfind(sb).map(|i| i as i32).unwrap_or(-1);
                        Some(Ok(Some(Value::Int(result))))
                    }
                    _ => Some(Err(JvmError::InvalidReference)),
                }
            }
            "compareTo" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    let result = match sa.cmp(sb) {
                        core::cmp::Ordering::Less => -1,
                        core::cmp::Ordering::Equal => 0,
                        core::cmp::Ordering::Greater => 1,
                    };
                    Some(Ok(Some(Value::Int(result))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },

            // ── String — allocating ──────────────────────────────────────
            "substring" => match ctx.args.first() {
                Some(Value::Reference(idx)) => {
                    // Copy bytes to owned storage first so the immutable borrow
                    // on ctx.strings ends before we call intern_dyn (mutable).
                    let owned: Result<alloc::vec::Vec<u8>, JvmError> = {
                        let s = ctx.strings.resolve(*idx).unwrap_or("");
                        let bytes = s.as_bytes();
                        let start = match ctx.args.get(1) {
                            Some(Value::Int(n)) => *n as usize,
                            _ => return Some(Err(JvmError::InvalidReference)),
                        };
                        let end = match ctx.args.get(2) {
                            Some(Value::Int(n)) => *n as usize,
                            None => bytes.len(),
                            _ => return Some(Err(JvmError::InvalidReference)),
                        };
                        if start > end || end > bytes.len() {
                            Err(JvmError::ArrayIndexOutOfBounds)
                        } else {
                            Ok(bytes[start..end].to_vec())
                        }
                    };
                    match owned {
                        Err(e) => Some(Err(e)),
                        Ok(v) => {
                            let r = ctx.strings.intern_dyn(&v).ok_or(JvmError::StackOverflow);
                            Some(r.map(|idx| Some(Value::Reference(idx))))
                        }
                    }
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "trim" => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let owned: alloc::vec::Vec<u8> = {
                        let s = ctx.strings.resolve(*idx).unwrap_or("");
                        s.trim_matches(|c: char| c.is_ascii_whitespace())
                            .as_bytes()
                            .to_vec()
                    };
                    let r = ctx
                        .strings
                        .intern_dyn(&owned)
                        .ok_or(JvmError::StackOverflow);
                    Some(r.map(|v| Some(Value::Reference(v))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            "toUpperCase" => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let upper: alloc::vec::Vec<u8> = {
                        let s = ctx.strings.resolve(*idx).unwrap_or("");
                        s.bytes().map(|b| b.to_ascii_uppercase()).collect()
                    };
                    let r = ctx
                        .strings
                        .intern_dyn(&upper)
                        .ok_or(JvmError::StackOverflow);
                    Some(r.map(|v| Some(Value::Reference(v))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            "toLowerCase" => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let lower: alloc::vec::Vec<u8> = {
                        let s = ctx.strings.resolve(*idx).unwrap_or("");
                        s.bytes().map(|b| b.to_ascii_lowercase()).collect()
                    };
                    let r = ctx
                        .strings
                        .intern_dyn(&lower)
                        .ok_or(JvmError::StackOverflow);
                    Some(r.map(|v| Some(Value::Reference(v))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            "valueOf" => {
                // Static method: String.valueOf(int/long/boolean/char/float/double)
                let result: Option<alloc::vec::Vec<u8>> = match ctx.args.first() {
                    Some(Value::Int(n)) => {
                        if ctx.descriptor.starts_with("(Z)") {
                            Some(if *n != 0 {
                                b"true".to_vec()
                            } else {
                                b"false".to_vec()
                            })
                        } else if ctx.descriptor.starts_with("(C)") {
                            Some(alloc::vec![(*n as u8).max(0x20)])
                        } else {
                            let mut tmp = [0u8; 12];
                            let bytes = crate::object_heap::int_to_decimal_buf(*n, &mut tmp);
                            Some(bytes.to_vec())
                        }
                    }
                    Some(Value::Long(n)) => {
                        let mut tmp = [0u8; 21];
                        let bytes = crate::object_heap::long_to_decimal_buf(*n, &mut tmp);
                        Some(bytes.to_vec())
                    }
                    Some(Value::Float(f)) => {
                        let mut tmp = [0u8; 32];
                        let bytes = crate::object_heap::float_to_str_buf(*f, &mut tmp);
                        Some(bytes.to_vec())
                    }
                    Some(Value::Double(d)) => {
                        let mut tmp = [0u8; 32];
                        let bytes = crate::object_heap::float_to_str_buf(*d as f32, &mut tmp);
                        Some(bytes.to_vec())
                    }
                    _ => None,
                };
                if let Some(bytes) = result {
                    let r = ctx
                        .strings
                        .intern_dyn(&bytes)
                        .ok_or(JvmError::StackOverflow);
                    Some(r.map(|v| Some(Value::Reference(v))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            _ => None,
        }
    }

    fn dispatch_arraylist(
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match method_name {
            "<init>" => {
                // <init>() or <init>(int initialCapacity) — capacity hint ignored.
                let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null)
                else {
                    return Some(Err(JvmError::InvalidReference));
                };
                let buf_idx = match ctx.objects.list_alloc() {
                    Some(i) => i,
                    None => return Some(Err(JvmError::StackOverflow)),
                };
                ctx.objects
                    .set_field(obj_idx, 0, Value::Int(buf_idx as i32));
                Some(Ok(None))
            }
            "add" => {
                let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                    Ok(i) => i,
                    Err(e) => return Some(Err(e)),
                };
                if ctx.descriptor.starts_with("(I") {
                    // add(int index, Object element) → void
                    let Value::Int(i) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    let v = ctx.args.get(2).copied().unwrap_or(Value::Null);
                    ctx.objects.list_insert(buf_idx, i as usize, v);
                    Some(Ok(None))
                } else {
                    // add(Object element) → boolean (always true)
                    let v = ctx.args.get(1).copied().unwrap_or(Value::Null);
                    ctx.objects.list_add(buf_idx, v);
                    Some(Ok(Some(Value::Int(1))))
                }
            }
            "get" => {
                let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                    Ok(i) => i,
                    Err(e) => return Some(Err(e)),
                };
                let Value::Int(i) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                    return Some(Err(JvmError::InvalidReference));
                };
                match ctx.objects.list_get(buf_idx, i as usize) {
                    Some(v) => Some(Ok(Some(v))),
                    None => Some(Err(JvmError::ArrayIndexOutOfBounds)),
                }
            }
            "size" => {
                let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                    Ok(i) => i,
                    Err(e) => return Some(Err(e)),
                };
                Some(Ok(Some(Value::Int(ctx.objects.list_len(buf_idx) as i32))))
            }
            "isEmpty" => {
                let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                    Ok(i) => i,
                    Err(e) => return Some(Err(e)),
                };
                Some(Ok(Some(Value::Int(
                    (ctx.objects.list_len(buf_idx) == 0) as i32,
                ))))
            }
            "set" => {
                let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                    Ok(i) => i,
                    Err(e) => return Some(Err(e)),
                };
                let Value::Int(i) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                    return Some(Err(JvmError::InvalidReference));
                };
                let v = ctx.args.get(2).copied().unwrap_or(Value::Null);
                let old = ctx
                    .objects
                    .list_set(buf_idx, i as usize, v)
                    .unwrap_or(Value::Null);
                Some(Ok(Some(old)))
            }
            "remove" => {
                let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                    Ok(i) => i,
                    Err(e) => return Some(Err(e)),
                };
                let Value::Int(i) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                    return Some(Err(JvmError::InvalidReference));
                };
                match ctx.objects.list_remove(buf_idx, i as usize) {
                    Some(v) => Some(Ok(Some(v))),
                    None => Some(Err(JvmError::ArrayIndexOutOfBounds)),
                }
            }
            "clear" => {
                let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                    Ok(i) => i,
                    Err(e) => return Some(Err(e)),
                };
                ctx.objects.list_clear(buf_idx);
                Some(Ok(None))
            }
            "contains" => {
                let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                    Ok(i) => i,
                    Err(e) => return Some(Err(e)),
                };
                let needle = ctx.args.get(1).copied().unwrap_or(Value::Null);
                let len = ctx.objects.list_len(buf_idx);
                let mut found = false;
                for i in 0..len {
                    let elem = ctx.objects.list_get(buf_idx, i).unwrap_or(Value::Null);
                    if values_eq(elem, needle, ctx.objects) {
                        found = true;
                        break;
                    }
                }
                Some(Ok(Some(Value::Int(found as i32))))
            }
            _ => None,
        }
    }

    fn dispatch_math(
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match method_name {
            "abs" => match ctx.args.first() {
                Some(Value::Int(i)) => Some(Ok(Some(Value::Int(i.abs())))),
                Some(Value::Long(l)) => Some(Ok(Some(Value::Long(l.abs())))),
                Some(Value::Float(f)) => Some(Ok(Some(Value::Float(f.abs())))),
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(d.abs())))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "min" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Int(a)), Some(Value::Int(b))) => Some(Ok(Some(Value::Int(*a.min(b))))),
                (Some(Value::Long(a)), Some(Value::Long(b))) => {
                    Some(Ok(Some(Value::Long(*a.min(b)))))
                }
                (Some(Value::Float(a)), Some(Value::Float(b))) => {
                    Some(Ok(Some(Value::Float(a.min(*b)))))
                }
                (Some(Value::Double(a)), Some(Value::Double(b))) => {
                    Some(Ok(Some(Value::Double(a.min(*b)))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "max" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Int(a)), Some(Value::Int(b))) => Some(Ok(Some(Value::Int(*a.max(b))))),
                (Some(Value::Long(a)), Some(Value::Long(b))) => {
                    Some(Ok(Some(Value::Long(*a.max(b)))))
                }
                (Some(Value::Float(a)), Some(Value::Float(b))) => {
                    Some(Ok(Some(Value::Float(a.max(*b)))))
                }
                (Some(Value::Double(a)), Some(Value::Double(b))) => {
                    Some(Ok(Some(Value::Double(a.max(*b)))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "sqrt" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::sqrt(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "pow" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Double(a)), Some(Value::Double(b))) => {
                    Some(Ok(Some(Value::Double(libm::pow(*a, *b)))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "floor" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::floor(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "ceil" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::ceil(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "round" => match ctx.args.first() {
                Some(Value::Float(f)) => Some(Ok(Some(Value::Int(libm::roundf(*f) as i32)))),
                Some(Value::Double(d)) => Some(Ok(Some(Value::Long(libm::round(*d) as i64)))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "sin" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::sin(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "cos" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::cos(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "tan" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::tan(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "atan2" => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Double(y)), Some(Value::Double(x))) => {
                    Some(Ok(Some(Value::Double(libm::atan2(*y, *x)))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "toRadians" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(
                    *d * (core::f64::consts::PI / 180.0),
                )))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "toDegrees" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(
                    *d * (180.0 / core::f64::consts::PI),
                )))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "log" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::log(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "log10" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::log10(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            "exp" => match ctx.args.first() {
                Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::exp(*d))))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            _ => None,
        }
    }
}

impl NativeMethodHandler for BuiltinHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match class_name {
            // ── Object / Exception / RuntimeException ────────────────────────
            "java/lang/Object" | "java/lang/Exception" | "java/lang/RuntimeException" => {
                match method_name {
                    "<init>" => Some(Ok(None)),
                    _ => None,
                }
            }

            // ── Throwable ───────────────────────────────────────────────────
            "java/lang/Throwable" => match method_name {
                "<init>" => Some(Ok(None)),
                // addSuppressed is called by try-with-resources when both the body
                // and close() throw.  Suppressed exceptions are not useful on an
                // embedded device, so we accept the call and discard the argument.
                "addSuppressed" => Some(Ok(None)),
                _ => None,
            },

            // ── StringBuilder ────────────────────────────────────────────────
            "java/lang/StringBuilder" => match method_name {
                "<init>" => {
                    ctx.objects.sb_clear();
                    // <init>(String): if a String argument was supplied, seed the buffer.
                    if let Some(Value::Reference(idx)) = ctx.args.get(1) {
                        let s = ctx.strings.resolve(*idx).unwrap_or("");
                        ctx.objects.sb_append_bytes(s.as_bytes());
                    }
                    Some(Ok(None))
                }
                "append" => {
                    match ctx.args.get(1) {
                        Some(Value::Reference(idx)) => {
                            let s = ctx.strings.resolve(*idx).unwrap_or("");
                            ctx.objects.sb_append_bytes(s.as_bytes());
                        }
                        Some(Value::Int(n)) => {
                            let desc = ctx.descriptor;
                            if desc.starts_with("(C)") {
                                // append(char): emit the character as a single byte.
                                // Multi-byte Unicode chars are not supported on this platform.
                                let ch = (*n as u8).max(0x20); // replace non-printable with space
                                ctx.objects.sb_append_bytes(&[ch]);
                            } else if desc.starts_with("(Z)") {
                                // append(boolean)
                                ctx.objects.sb_append_bytes(if *n != 0 {
                                    b"true"
                                } else {
                                    b"false"
                                });
                            } else {
                                ctx.objects.sb_append_int(*n);
                            }
                        }
                        Some(Value::Long(n)) => {
                            ctx.objects.sb_append_long(*n);
                        }
                        Some(Value::Float(f)) => {
                            ctx.objects.sb_append_float(*f);
                        }
                        Some(Value::Double(d)) => {
                            ctx.objects.sb_append_float(*d as f32);
                        }
                        _ => {}
                    }
                    // append() returns `this` for chaining.
                    Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None)))
                }
                "length" => {
                    let len = ctx.objects.sb_len() as i32;
                    Some(Ok(Some(Value::Int(len))))
                }
                "charAt" => {
                    if let Some(Value::Int(i)) = ctx.args.get(1) {
                        let ch = ctx.objects.sb_char_at(*i as usize).unwrap_or(0);
                        Some(Ok(Some(Value::Int(ch as i32))))
                    } else {
                        Some(Err(JvmError::InvalidReference))
                    }
                }
                "toString" => {
                    let bytes = ctx.objects.sb_contents_slice().to_vec();
                    let str_ref = ctx
                        .strings
                        .intern_dyn(&bytes)
                        .ok_or(JvmError::StackOverflow);
                    Some(str_ref.map(|r| Some(Value::Reference(r))))
                }
                _ => None,
            },

            // ── String ──────────────────────────────────────────────────────
            "java/lang/String" => Self::dispatch_string(method_name, ctx),

            // ── Integer ──────────────────────────────────────────────────────
            "java/lang/Integer" => match method_name {
                "<init>" => {
                    // new Integer(int) — deprecated form; store value in field 0.
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    let val = ctx.args.get(1).copied().unwrap_or(Value::Null);
                    ctx.objects.set_field(obj, 0, val);
                    Some(Ok(None))
                }
                "valueOf" => {
                    // static Integer.valueOf(int) — autoboxing entry point.
                    let val = ctx.args.first().copied().unwrap_or(Value::Null);
                    let obj_idx = ctx
                        .objects
                        .alloc("java/lang/Integer")
                        .ok_or(JvmError::StackOverflow);
                    match obj_idx {
                        Err(e) => Some(Err(e)),
                        Ok(idx) => {
                            ctx.objects.set_field(idx, 0, val);
                            Some(Ok(Some(Value::ObjectRef(idx))))
                        }
                    }
                }
                "intValue" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    Some(Ok(Some(
                        ctx.objects.get_field(obj, 0).unwrap_or(Value::Int(0)),
                    )))
                }
                _ => None,
            },

            // ── Boolean ──────────────────────────────────────────────────────
            "java/lang/Boolean" => match method_name {
                "<init>" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    let val = ctx.args.get(1).copied().unwrap_or(Value::Null);
                    ctx.objects.set_field(obj, 0, val);
                    Some(Ok(None))
                }
                "valueOf" => {
                    let val = ctx.args.first().copied().unwrap_or(Value::Null);
                    let obj_idx = ctx
                        .objects
                        .alloc("java/lang/Boolean")
                        .ok_or(JvmError::StackOverflow);
                    match obj_idx {
                        Err(e) => Some(Err(e)),
                        Ok(idx) => {
                            ctx.objects.set_field(idx, 0, val);
                            Some(Ok(Some(Value::ObjectRef(idx))))
                        }
                    }
                }
                "booleanValue" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    Some(Ok(Some(
                        ctx.objects.get_field(obj, 0).unwrap_or(Value::Int(0)),
                    )))
                }
                _ => None,
            },

            // ── Long ─────────────────────────────────────────────────────────
            "java/lang/Long" => match method_name {
                "<init>" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    let val = ctx.args.get(1).copied().unwrap_or(Value::Null);
                    ctx.objects.set_field(obj, 0, val);
                    Some(Ok(None))
                }
                "valueOf" => {
                    let val = ctx.args.first().copied().unwrap_or(Value::Null);
                    let obj_idx = ctx
                        .objects
                        .alloc("java/lang/Long")
                        .ok_or(JvmError::StackOverflow);
                    match obj_idx {
                        Err(e) => Some(Err(e)),
                        Ok(idx) => {
                            ctx.objects.set_field(idx, 0, val);
                            Some(Ok(Some(Value::ObjectRef(idx))))
                        }
                    }
                }
                "longValue" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    Some(Ok(Some(
                        ctx.objects.get_field(obj, 0).unwrap_or(Value::Long(0)),
                    )))
                }
                _ => None,
            },

            // ── Float ────────────────────────────────────────────────────────
            "java/lang/Float" => match method_name {
                "<init>" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    let val = ctx.args.get(1).copied().unwrap_or(Value::Null);
                    ctx.objects.set_field(obj, 0, val);
                    Some(Ok(None))
                }
                "valueOf" => {
                    let val = ctx.args.first().copied().unwrap_or(Value::Null);
                    let obj_idx = ctx
                        .objects
                        .alloc("java/lang/Float")
                        .ok_or(JvmError::StackOverflow);
                    match obj_idx {
                        Err(e) => Some(Err(e)),
                        Ok(idx) => {
                            ctx.objects.set_field(idx, 0, val);
                            Some(Ok(Some(Value::ObjectRef(idx))))
                        }
                    }
                }
                "floatValue" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    Some(Ok(Some(
                        ctx.objects.get_field(obj, 0).unwrap_or(Value::Float(0.0)),
                    )))
                }
                _ => None,
            },

            // ── Double ───────────────────────────────────────────────────────
            "java/lang/Double" => match method_name {
                "<init>" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    let val = ctx.args.get(1).copied().unwrap_or(Value::Null);
                    ctx.objects.set_field(obj, 0, val);
                    Some(Ok(None))
                }
                "valueOf" => {
                    let val = ctx.args.first().copied().unwrap_or(Value::Null);
                    let obj_idx = ctx
                        .objects
                        .alloc("java/lang/Double")
                        .ok_or(JvmError::StackOverflow);
                    match obj_idx {
                        Err(e) => Some(Err(e)),
                        Ok(idx) => {
                            ctx.objects.set_field(idx, 0, val);
                            Some(Ok(Some(Value::ObjectRef(idx))))
                        }
                    }
                }
                "doubleValue" => {
                    let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null)
                    else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    Some(Ok(Some(
                        ctx.objects.get_field(obj, 0).unwrap_or(Value::Double(0.0)),
                    )))
                }
                _ => None,
            },

            // ── ArrayList ────────────────────────────────────────────────────
            "java/util/ArrayList" => Self::dispatch_arraylist(method_name, ctx),

            // ── Math ─────────────────────────────────────────────────────────
            "java/lang/Math" => Self::dispatch_math(method_name, ctx),

            _ => None,
        }
    }
}

#[cfg(test)]
mod math_tests {
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
}
