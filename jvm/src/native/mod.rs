use crate::{
    array_heap::ArrayHeap,
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

mod boxed;
mod collections;
mod math;
mod string;
mod string_builder;

#[cfg(test)]
mod tests;

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

    /// Returns platform monotonic clock in nanoseconds.
    ///
    /// Used by the interpreter to measure GC pause times.  The default
    /// returns `0` (no timing); override on platforms that have a clock.
    fn clock_nanos(&self) -> u64 {
        0
    }

    /// Called by the interpreter after each GC cycle.
    ///
    /// `time_ns` is the wall-clock time spent in the collector (from
    /// [`clock_nanos`](NativeMethodHandler::clock_nanos)) and `freed` is the
    /// number of heap entries reclaimed.  The default is a no-op.
    fn report_gc(&mut self, _time_ns: u64, _freed: usize) {}
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

impl NativeMethodHandler for BuiltinHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match class_name {
            "java/lang/Object" | "java/lang/Exception" | "java/lang/RuntimeException" => {
                match method_name {
                    "<init>" => Some(Ok(None)),
                    _ => None,
                }
            }
            "java/lang/Throwable" => match method_name {
                "<init>" => Some(Ok(None)),
                "addSuppressed" => Some(Ok(None)),
                _ => None,
            },
            "java/lang/StringBuilder" => string_builder::dispatch(method_name, ctx),
            "java/lang/String" => string::dispatch(method_name, ctx),
            "java/lang/Integer" => boxed::dispatch_integer(method_name, ctx),
            "java/lang/Boolean" => boxed::dispatch_boolean(method_name, ctx),
            "java/lang/Long" => boxed::dispatch_long(method_name, ctx),
            "java/lang/Float" => boxed::dispatch_float(method_name, ctx),
            "java/lang/Double" => boxed::dispatch_double(method_name, ctx),
            "java/util/ArrayList" => collections::dispatch(method_name, ctx),
            "java/lang/Math" => math::dispatch(method_name, ctx),
            _ => None,
        }
    }
}
