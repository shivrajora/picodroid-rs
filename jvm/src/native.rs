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
/// | `java/lang/StringBuilder` | `<init>`, `append(String)`, `append(int)`, `append(char)`, `append(long)`, `append(double)`, `toString` |
/// | `java/lang/String` | `length`, `charAt` |
pub struct BuiltinHandler;

impl NativeMethodHandler for BuiltinHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match (class_name, method_name) {
            ("java/lang/Object", "<init>")
            | ("java/lang/Throwable", "<init>")
            | ("java/lang/Exception", "<init>")
            | ("java/lang/RuntimeException", "<init>") => Some(Ok(None)),

            ("java/lang/StringBuilder", "<init>") => {
                ctx.objects.sb_clear();
                Some(Ok(None))
            }
            ("java/lang/StringBuilder", "append") => {
                match ctx.args.get(1) {
                    Some(Value::Reference(idx)) => {
                        let s = ctx.strings.resolve(*idx).unwrap_or("");
                        ctx.objects.sb_append_bytes(s.as_bytes());
                    }
                    Some(Value::Int(n)) => {
                        if ctx.descriptor.starts_with("(C)") {
                            // append(char): emit the character, not its decimal value
                            let ch = (*n as u8).max(0x20); // replace non-printable with space
                            ctx.objects.sb_append_bytes(&[ch]);
                        } else {
                            ctx.objects.sb_append_int(*n);
                        }
                    }
                    Some(Value::Long(n)) => {
                        ctx.objects.sb_append_long(*n);
                    }
                    Some(Value::Double(d)) => {
                        ctx.objects.sb_append_int(*d as i32);
                    }
                    _ => {}
                }
                Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None)))
            }
            ("java/lang/StringBuilder", "toString") => {
                let (ptr, len) = ctx.objects.sb_contents();
                // SAFETY: ptr points into ObjectHeap::sb_buf which lives for the
                // duration of the JVM. The dyn_slot in StringTable is overwritten on
                // the next toString() call, which is safe because Log.i (the only
                // consumer) copies the string before that happens.
                let str_ref =
                    unsafe { ctx.strings.intern_dyn(ptr, len) }.ok_or(JvmError::StackOverflow);
                Some(str_ref.map(|r| Some(Value::Reference(r))))
            }

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

            _ => None,
        }
    }
}
