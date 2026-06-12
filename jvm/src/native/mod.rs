// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    array_heap::ArrayHeap,
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, MonitorKey, Value},
};

mod arrays;
mod boxed;
mod class_obj;
mod collections;
mod enumeration;
mod hashmap;
mod hashset;
mod iterator;
mod math;
mod random;
mod string;
mod string_builder;
mod string_format;

#[cfg(test)]
mod tests;

/// Per-class dispatch function used by [`BuiltinHandler`].
type BuiltinDispatchFn =
    fn(method_name: &str, ctx: &mut NativeContext<'_>) -> Option<Result<Option<Value>, JvmError>>;

/// Every native class name the JVM canonicalises to a `&'static str` for
/// pointer-identity caching. A class that appears in `BUILTIN_DISPATCH` MUST
/// also appear here (the `builtin_dispatch_classes_subset_of_names` test
/// enforces this).
///
/// Two kinds of entries appear:
/// - **Dispatched builtins** (`java/lang/String`, `java/util/HashMap`, ...) —
///   handled by [`BuiltinHandler`] when the user-supplied
///   [`NativeMethodHandler`] passes the call through.
/// - **Canonicalisation-only** (`java/lang/System`, `java/lang/Runnable`) —
///   handled by the user's [`NativeMethodHandler`]. They appear here only so
///   the interpreter can produce a stable `&'static str` for caching, not
///   because `BuiltinHandler` knows what to do with them.
pub const BUILTIN_CLASS_NAMES: &[&str] = &[
    // Dispatched builtins (kept in lockstep with BUILTIN_DISPATCH below).
    "java/lang/Object",
    "java/lang/Class",
    "java/lang/Throwable",
    "java/lang/Exception",
    "java/lang/RuntimeException",
    "java/util/IllegalFormatException",
    "java/lang/Enum",
    "java/lang/StringBuilder",
    "java/lang/String",
    "java/lang/Integer",
    "java/lang/Boolean",
    "java/lang/Long",
    "java/lang/Float",
    "java/lang/Double",
    "java/lang/Character",
    "java/lang/Byte",
    "java/lang/Short",
    "java/util/ArrayList",
    "java/util/HashMap",
    "java/util/HashMap$KeySet",
    "java/util/HashMap$Values",
    "java/util/HashSet",
    "java/util/Iterator",
    "java/util/Random",
    "java/util/Arrays",
    "java/lang/Math",
    // Canonicalisation-only — handled by the user's NativeMethodHandler or
    // referenced as interface/superclass names in user code.
    "java/lang/System",
    "java/lang/Runnable",
    "java/util/Collections",
    "java/util/List",
    "java/lang/Comparable",
];

/// Table consulted by [`BuiltinHandler::dispatch`]. Single source of truth:
/// changing this table changes the dispatch behaviour. The
/// `builtin_dispatch_classes_subset_of_names` test asserts every class here is
/// also in [`BUILTIN_CLASS_NAMES`] so canonicalisation cannot drift.
const BUILTIN_DISPATCH: &[(&str, BuiltinDispatchFn)] = &[
    ("java/lang/Object", dispatch_object),
    ("java/lang/Class", class_obj::dispatch),
    ("java/lang/Throwable", dispatch_throwable),
    ("java/lang/Exception", dispatch_init_only),
    ("java/lang/RuntimeException", dispatch_init_only),
    ("java/util/IllegalFormatException", dispatch_init_only),
    ("java/lang/Enum", enumeration::dispatch),
    ("java/lang/StringBuilder", string_builder::dispatch),
    ("java/lang/String", string::dispatch),
    ("java/lang/Integer", boxed::dispatch_integer),
    ("java/lang/Boolean", boxed::dispatch_boolean),
    ("java/lang/Long", boxed::dispatch_long),
    ("java/lang/Float", boxed::dispatch_float),
    ("java/lang/Double", boxed::dispatch_double),
    ("java/lang/Character", boxed::dispatch_character),
    ("java/lang/Byte", boxed::dispatch_byte),
    ("java/lang/Short", boxed::dispatch_short),
    ("java/util/ArrayList", collections::dispatch),
    ("java/util/HashMap", hashmap::dispatch),
    ("java/util/HashMap$KeySet", hashmap::dispatch_keyset),
    ("java/util/HashMap$Values", hashmap::dispatch_values),
    ("java/util/HashSet", hashset::dispatch),
    ("java/util/Iterator", iterator::dispatch),
    ("java/util/Random", random::dispatch),
    ("java/util/Arrays", arrays::dispatch),
    ("java/lang/Math", math::dispatch),
    // System is otherwise canonicalisation-only (currentTimeMillis lives in
    // the platform handler, which dispatches first); arraycopy is pure array
    // machinery, so it belongs to the builtins.
    ("java/lang/System", arrays::dispatch_system),
];

/// If the receiver is a Throwable being constructed with a String first arg
/// (e.g. `<init>(Ljava/lang/String;)V`), record that message in the side
/// table on the ObjectHeap so it can later be surfaced in `UncaughtException`.
fn capture_throwable_message(ctx: &mut NativeContext<'_>) {
    if !ctx.descriptor.starts_with("(Ljava/lang/String;") {
        return;
    }
    let Some(Value::ObjectRef(obj_idx)) = ctx.args.first().copied() else {
        return;
    };
    let Some(Value::Reference(msg_idx)) = ctx.args.get(1).copied() else {
        return;
    };
    ctx.objects.register_exception_message(obj_idx, msg_idx);
}

/// `getMessage()` on any Throwable-family receiver: surface the message
/// recorded by [`capture_throwable_message`] at construction, or `null` for
/// exceptions built without a String message — Android's exact contract.
fn throwable_get_message(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let Some(Value::ObjectRef(obj_idx)) = ctx.args.first().copied() else {
        return Err(JvmError::InvalidReference);
    };
    Ok(Some(match ctx.objects.get_exception_message(obj_idx) {
        Some(msg_idx) => Value::Reference(msg_idx),
        None => Value::Null,
    }))
}

fn dispatch_init_only(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            capture_throwable_message(ctx);
            Some(Ok(None))
        }
        "getMessage" => Some(throwable_get_message(ctx)),
        _ => None,
    }
}

/// `java/lang/Object` dispatcher: `<init>` plus a `toString` that mirrors Java's
/// `String.toString()` (returns `this`). Because the builtin `String` has no
/// class-file method table, an `invokevirtual toString()` on a value whose
/// static type is `Object`/`String` resolves to `Object.toString` here — so
/// returning the receiver unchanged when it is already a string reference makes
/// the idiomatic `Adapter`/`AdapterView` item rendering (`getItem(i).toString()`)
/// and `"" + obj` work. Non-string receivers keep the previous behaviour
/// (unimplemented → caller sees `NoSuchMethod`).
fn dispatch_object(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            capture_throwable_message(ctx);
            Some(Ok(None))
        }
        "toString" => match ctx.args.first() {
            Some(Value::Reference(idx)) => Some(Ok(Some(Value::Reference(*idx)))),
            _ => None,
        },
        _ => None,
    }
}

fn dispatch_throwable(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            capture_throwable_message(ctx);
            Some(Ok(None))
        }
        "getMessage" => Some(throwable_get_message(ctx)),
        "addSuppressed" => Some(Ok(None)),
        _ => None,
    }
}

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
    /// [`clock_nanos`](NativeMethodHandler::clock_nanos)), `freed` is the
    /// number of heap entries reclaimed, and `pre_gc_used` is the approximate
    /// live-bytes total across object / array / string heaps *before* the
    /// sweep ran — handlers can use this to update a peak-heap counter
    /// (since GC is triggered at high-water moments). The default is a no-op.
    fn report_gc(&mut self, _time_ns: u64, _freed: usize, _pre_gc_used: usize) {}

    /// Acquire the monitor associated with `key` (Java `monitorenter`).
    ///
    /// If the current thread already owns the monitor, the implementation must
    /// support reentrant locking (increment an internal count).  If another
    /// thread holds the monitor, the implementation should block until it is
    /// released.
    ///
    /// The default is a no-op, which is correct for single-threaded
    /// environments (simulator, unit tests).
    fn monitor_enter(&mut self, _key: MonitorKey) -> Result<(), JvmError> {
        Ok(())
    }

    /// Release the monitor associated with `key` (Java `monitorexit`).
    ///
    /// Decrements the reentrant lock count; when it reaches zero the monitor
    /// is fully released and other threads may acquire it.
    ///
    /// The default is a no-op, which is correct for single-threaded
    /// environments (simulator, unit tests).
    fn monitor_exit(&mut self, _key: MonitorKey) -> Result<(), JvmError> {
        Ok(())
    }

    /// Drop all monitor state.
    ///
    /// Called when the JVM heap is reset (e.g. before running a new app).
    /// Implementations should release any OS-level mutex resources.
    fn monitors_clear(&mut self) {}

    /// Visit object / array / string references held in native state so the
    /// GC keeps them alive across cycles.
    ///
    /// Without this, refs the handler keeps in its own state (Activity
    /// stack, sensor registrations, service bindings, etc.) are invisible
    /// to the mark phase and get swept the moment they fall off the Java
    /// frame stack. This bites callback-driven apps hardest: between two
    /// `onSensorChanged` calls the only reference to the Activity might be
    /// in the handler's activity-stack entry, and a GC in that gap will
    /// collect it.
    ///
    /// Implementations call `visit(Value::ObjectRef(idx))` (or `ArrayRef`,
    /// `Reference`) for every reference they own. Non-reference `Value`
    /// kinds are ignored. The callback is zero-alloc; do not buffer.
    ///
    /// Default is a no-op (handlers with no retained refs need nothing).
    fn gc_visit_roots(&self, _visit: &mut dyn FnMut(Value)) {}

    /// Names of the native classes this handler dispatches.
    ///
    /// The interpreter consults this list (in addition to the JVM's own
    /// [`BUILTIN_CLASS_NAMES`]) when canonicalising a class name to the
    /// `&'static str` used as a pointer-identity cache key. Without an entry
    /// here, virtual dispatch on a native class will silently fall back to
    /// `"unknown"`.
    ///
    /// Return a `&'static [&'static str]` const declared by your crate.
    /// Default returns `&[]` (no extra native classes).
    fn native_class_names(&self) -> &'static [&'static str] {
        &[]
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
/// | `java/lang/String` | `length`, `charAt`, `equals`, `equalsIgnoreCase`, `startsWith`, `endsWith`, `contains`, `indexOf`, `lastIndexOf`, `isEmpty`, `compareTo`, `substring`, `trim`, `toUpperCase`, `toLowerCase`, `valueOf`, `concat`, `hashCode`, `toCharArray`, `replace`, `split` |
/// | `java/lang/Integer` | `<init>`, `valueOf`, `intValue` |
/// | `java/lang/Boolean` | `<init>`, `valueOf`, `booleanValue` |
/// | `java/lang/Long` | `<init>`, `valueOf`, `longValue` |
/// | `java/lang/Float` | `<init>`, `valueOf`, `floatValue` |
/// | `java/lang/Double` | `<init>`, `valueOf`, `doubleValue` |
/// | `java/util/ArrayList` | `<init>`, `add`, `get`, `size`, `isEmpty`, `set`, `remove`, `clear`, `contains` |
/// | `java/util/HashMap` | `<init>`, `put`, `get`, `remove`, `containsKey`, `containsValue`, `size`, `isEmpty`, `clear`, `getOrDefault`, `keySet`, `values` |
/// | `java/util/HashSet` | `<init>`, `add`, `remove`, `contains`, `size`, `isEmpty`, `clear` |
/// | `java/util/Iterator` | `hasNext`, `next` |
/// | `java/util/Random` | `<init>`, `<init>(long)`, `setSeed`, `nextInt`, `nextInt(int)`, `nextLong`, `nextBoolean`, `nextFloat`, `nextDouble`, `nextGaussian`, `nextBytes` |
/// | `java/util/Arrays` | `sort`, `fill`, `copyOf`, `toString` (all numeric primitive overloads: int/long/double/float/short/byte/char) |
/// | `java/lang/Enum` | `<init>`, `name`, `ordinal`, `toString`, `equals`, `compareTo` |
/// | `java/lang/Math` | `abs`, `min`, `max`, `sqrt`, `pow`, `floor`, `ceil`, `round`, `sin`, `cos`, `tan`, `atan2`, `toRadians`, `toDegrees`, `log`, `log10`, `exp` |
pub struct BuiltinHandler;

impl NativeMethodHandler for BuiltinHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        // Array clone: class name starts with '[' and method is "clone".
        // Needed for enum Color.values() which clones the internal $VALUES array.
        if class_name.starts_with('[') && method_name == "clone" {
            if let Some(Value::ArrayRef(idx)) = ctx.args.first().copied() {
                return Some(
                    ctx.arrays
                        .clone(idx)
                        .map(|new_idx| Some(Value::ArrayRef(new_idx)))
                        .ok_or(JvmError::StackOverflow),
                );
            }
            return Some(Err(JvmError::InvalidReference));
        }
        for &(name, dispatch_fn) in BUILTIN_DISPATCH {
            if name == class_name {
                return dispatch_fn(method_name, ctx);
            }
        }
        None
    }
}
