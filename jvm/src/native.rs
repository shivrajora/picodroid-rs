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
/// | `java/lang/Throwable` | `<init>` |
/// | `java/lang/Exception` | `<init>` |
/// | `java/lang/RuntimeException` | `<init>` |
/// | `java/lang/StringBuilder` | `<init>`, `<init>(String)`, `append(String/int/char/long/float/double/boolean)`, `length`, `charAt`, `toString` |
/// | `java/lang/String` | `length`, `charAt`, `equals`, `equalsIgnoreCase`, `startsWith`, `endsWith`, `contains`, `indexOf`, `lastIndexOf`, `isEmpty`, `compareTo`, `substring`, `trim`, `toUpperCase`, `toLowerCase`, `valueOf` |
pub struct BuiltinHandler;

impl NativeMethodHandler for BuiltinHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match (class_name, method_name) {
            // ── Object hierarchy constructors ────────────────────────────────
            ("java/lang/Object", "<init>")
            | ("java/lang/Throwable", "<init>")
            | ("java/lang/Exception", "<init>")
            | ("java/lang/RuntimeException", "<init>") => Some(Ok(None)),

            // ── StringBuilder ────────────────────────────────────────────────
            ("java/lang/StringBuilder", "<init>") => {
                ctx.objects.sb_clear();
                // <init>(String): if a String argument was supplied, seed the buffer.
                if let Some(Value::Reference(idx)) = ctx.args.get(1) {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    ctx.objects.sb_append_bytes(s.as_bytes());
                }
                Some(Ok(None))
            }
            ("java/lang/StringBuilder", "append") => {
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
                            ctx.objects
                                .sb_append_bytes(if *n != 0 { b"true" } else { b"false" });
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
            ("java/lang/StringBuilder", "length") => {
                let len = ctx.objects.sb_len() as i32;
                Some(Ok(Some(Value::Int(len))))
            }
            ("java/lang/StringBuilder", "charAt") => {
                if let Some(Value::Int(i)) = ctx.args.get(1) {
                    let ch = ctx.objects.sb_char_at(*i as usize).unwrap_or(0);
                    Some(Ok(Some(Value::Int(ch as i32))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            ("java/lang/StringBuilder", "toString") => {
                let bytes = ctx.objects.sb_contents_slice().to_vec();
                let str_ref = ctx
                    .strings
                    .intern_dyn(&bytes)
                    .ok_or(JvmError::StackOverflow);
                Some(str_ref.map(|r| Some(Value::Reference(r))))
            }

            // ── String — non-allocating ──────────────────────────────────────
            ("java/lang/String", "length") => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    Some(Ok(Some(Value::Int(s.len() as i32))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            ("java/lang/String", "charAt") => {
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
            ("java/lang/String", "isEmpty") => {
                if let Some(Value::Reference(idx)) = ctx.args.first() {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    Some(Ok(Some(Value::Int(s.is_empty() as i32))))
                } else {
                    Some(Err(JvmError::InvalidReference))
                }
            }
            ("java/lang/String", "equals") => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int((sa == sb) as i32))))
                }
                (Some(Value::Reference(_)), Some(Value::Null)) => Some(Ok(Some(Value::Int(0)))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            ("java/lang/String", "equalsIgnoreCase") => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int(sa.eq_ignore_ascii_case(sb) as i32))))
                }
                (Some(Value::Reference(_)), Some(Value::Null)) => Some(Ok(Some(Value::Int(0)))),
                _ => Some(Err(JvmError::InvalidReference)),
            },
            ("java/lang/String", "startsWith") => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int(sa.starts_with(sb) as i32))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            ("java/lang/String", "endsWith") => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int(sa.ends_with(sb) as i32))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            ("java/lang/String", "contains") => match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    Some(Ok(Some(Value::Int(sa.contains(sb) as i32))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            },
            ("java/lang/String", "indexOf") => {
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
            ("java/lang/String", "lastIndexOf") => {
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
            ("java/lang/String", "compareTo") => match (ctx.args.first(), ctx.args.get(1)) {
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

            // ── String — allocating ──────────────────────────────────────────
            ("java/lang/String", "substring") => match ctx.args.first() {
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
            ("java/lang/String", "trim") => {
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
            ("java/lang/String", "toUpperCase") => {
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
            ("java/lang/String", "toLowerCase") => {
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
            ("java/lang/String", "valueOf") => {
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
}
