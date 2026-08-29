// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    array_heap::ArrayHeap,
    class_file::ClassFile,
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
    "java/util/HashMap$EntrySet",
    "java/util/Map$Entry",
    "java/util/HashSet",
    "java/util/LinkedHashMap",
    "java/util/LinkedHashSet",
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
    "java/util/Comparator",
    "java/lang/Cloneable",
    // Classfile-less classes that user code may `new`, `checkcast` or
    // `instanceof` (every name in the interpreter's `BUILTIN_SUPER` /
    // `BUILTIN_INTERFACES` tables). A `new` of a name missing here yields
    // an `"unknown"`-class object that no catch clause ever matches.
    "java/lang/Number",
    "java/lang/CharSequence",
    "java/lang/Appendable",
    "java/lang/Iterable",
    "java/util/Collection",
    "java/util/Set",
    "java/util/Map",
    "java/lang/Error",
    "java/lang/IllegalArgumentException",
    "java/lang/IllegalStateException",
    "java/lang/NullPointerException",
    "java/lang/ArithmeticException",
    "java/lang/ClassCastException",
    "java/lang/UnsupportedOperationException",
    "java/lang/IndexOutOfBoundsException",
    "java/lang/ArrayIndexOutOfBoundsException",
    "java/lang/StringIndexOutOfBoundsException",
    "java/lang/NumberFormatException",
    "java/lang/ExceptionInInitializerError",
    "java/lang/StackOverflowError",
    "java/util/NoSuchElementException",
    "java/io/IOException",
    "java/io/InterruptedIOException",
    "java/net/SocketTimeoutException",
    "java/net/SocketException",
    "java/net/ConnectException",
    "java/net/NoRouteToHostException",
    "java/net/BindException",
    "java/net/UnknownHostException",
    "java/net/ProtocolException",
];

/// Every `(declaring class, method, descriptor)` the built-in handler serves
/// for a class the *embedder's* SDK declares `native` — the JVM-side half of
/// the method-level dispatch cross-check (audit P1-6). The platform test in
/// `native_handler/method_tables.rs` unions this with its own per-module
/// tables and diffs the result against the SDK's `ACC_NATIVE` methods in both
/// directions, so a typo here or a missing arm below surfaces at build time
/// instead of as a runtime `NoSuchMethod`.
///
/// Only rows whose class an SDK can plausibly declare `native` belong here
/// (`java/util/Arrays`, `java/lang/Math`, …); internal builtins like
/// `java/lang/String`, whose methods are implemented rather than declared
/// `native` by the SDK, have no class file and are outside the diff by
/// construction.
pub const BUILTIN_SDK_HANDLED: &[(&str, &str, &str)] = &[
    // java/lang/Class
    ("java/lang/Class", "getName", "()Ljava/lang/String;"),
    // java/lang/Math
    ("java/lang/Math", "abs", "(D)D"),
    ("java/lang/Math", "abs", "(F)F"),
    ("java/lang/Math", "abs", "(I)I"),
    ("java/lang/Math", "abs", "(J)J"),
    ("java/lang/Math", "atan2", "(DD)D"),
    ("java/lang/Math", "ceil", "(D)D"),
    ("java/lang/Math", "cos", "(D)D"),
    ("java/lang/Math", "exp", "(D)D"),
    ("java/lang/Math", "floor", "(D)D"),
    ("java/lang/Math", "log", "(D)D"),
    ("java/lang/Math", "log10", "(D)D"),
    ("java/lang/Math", "max", "(DD)D"),
    ("java/lang/Math", "max", "(FF)F"),
    ("java/lang/Math", "max", "(II)I"),
    ("java/lang/Math", "max", "(JJ)J"),
    ("java/lang/Math", "min", "(DD)D"),
    ("java/lang/Math", "min", "(FF)F"),
    ("java/lang/Math", "min", "(II)I"),
    ("java/lang/Math", "min", "(JJ)J"),
    ("java/lang/Math", "pow", "(DD)D"),
    ("java/lang/Math", "round", "(D)J"),
    ("java/lang/Math", "round", "(F)I"),
    ("java/lang/Math", "sin", "(D)D"),
    ("java/lang/Math", "sqrt", "(D)D"),
    ("java/lang/Math", "tan", "(D)D"),
    ("java/lang/Math", "toDegrees", "(D)D"),
    ("java/lang/Math", "toRadians", "(D)D"),
    // java/lang/System
    (
        "java/lang/System",
        "arraycopy",
        "(Ljava/lang/Object;ILjava/lang/Object;II)V",
    ),
    // java/util/Arrays
    ("java/util/Arrays", "copyOf", "([BI)[B"),
    ("java/util/Arrays", "copyOf", "([CI)[C"),
    ("java/util/Arrays", "copyOf", "([DI)[D"),
    ("java/util/Arrays", "copyOf", "([FI)[F"),
    ("java/util/Arrays", "copyOf", "([II)[I"),
    ("java/util/Arrays", "copyOf", "([JI)[J"),
    ("java/util/Arrays", "copyOf", "([SI)[S"),
    ("java/util/Arrays", "fill", "([BB)V"),
    ("java/util/Arrays", "fill", "([CC)V"),
    ("java/util/Arrays", "fill", "([DD)V"),
    ("java/util/Arrays", "fill", "([FF)V"),
    ("java/util/Arrays", "fill", "([II)V"),
    ("java/util/Arrays", "fill", "([JJ)V"),
    ("java/util/Arrays", "fill", "([SS)V"),
    ("java/util/Arrays", "sort", "([B)V"),
    ("java/util/Arrays", "sort", "([C)V"),
    ("java/util/Arrays", "sort", "([D)V"),
    ("java/util/Arrays", "sort", "([F)V"),
    ("java/util/Arrays", "sort", "([I)V"),
    ("java/util/Arrays", "sort", "([J)V"),
    ("java/util/Arrays", "sort", "([S)V"),
    ("java/util/Arrays", "toString", "([B)Ljava/lang/String;"),
    ("java/util/Arrays", "toString", "([C)Ljava/lang/String;"),
    ("java/util/Arrays", "toString", "([D)Ljava/lang/String;"),
    ("java/util/Arrays", "toString", "([F)Ljava/lang/String;"),
    ("java/util/Arrays", "toString", "([I)Ljava/lang/String;"),
    ("java/util/Arrays", "toString", "([J)Ljava/lang/String;"),
    ("java/util/Arrays", "toString", "([S)Ljava/lang/String;"),
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
    ("java/util/HashMap$KeySet", hashmap::dispatch_view),
    ("java/util/HashMap$Values", hashmap::dispatch_view),
    ("java/util/HashMap$EntrySet", hashmap::dispatch_view),
    ("java/util/Map$Entry", hashmap::dispatch_entry),
    ("java/util/HashSet", hashset::dispatch),
    // Insertion-ordered aliases (documented divergence: hash order). The
    // no-arg `mutableMapOf()`/`mutableSetOf()` are inline in Kotlin and emit
    // `new java/util/LinkedHashMap` at the call site.
    ("java/util/LinkedHashMap", hashmap::dispatch),
    ("java/util/LinkedHashSet", hashset::dispatch),
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

/// Allocate `class` and wrap it as a thrown Java exception (the pattern
/// established for NumberFormatException: alloc-by-name; exact-name catch
/// works).
pub(super) fn throw_named(ctx: &mut NativeContext<'_>, class: &'static str) -> JvmError {
    match ctx.objects.alloc(class) {
        Some(idx) => JvmError::Exception(idx),
        None => JvmError::StackOverflow,
    }
}

/// `Throwable.addSuppressed(Throwable)`: record in the ObjectHeap side table.
fn throwable_add_suppressed(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let Some(Value::ObjectRef(owner)) = ctx.args.first().copied() else {
        return Err(JvmError::InvalidReference);
    };
    match ctx.args.get(1).copied() {
        Some(Value::ObjectRef(t)) if t == owner => {
            Err(throw_named(ctx, "java/lang/IllegalArgumentException"))
        }
        Some(Value::ObjectRef(t)) => {
            ctx.objects.add_suppressed(owner, t);
            Ok(None)
        }
        Some(Value::Null) | None => Err(throw_named(ctx, "java/lang/NullPointerException")),
        Some(_) => Err(JvmError::InvalidReference),
    }
}

/// `Throwable.getSuppressed()`: the recorded Throwables as a `Throwable[]`
/// (empty array when none — never null, per the Java contract).
fn throwable_get_suppressed(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let Some(Value::ObjectRef(owner)) = ctx.args.first().copied() else {
        return Err(JvmError::InvalidReference);
    };
    let list: alloc::vec::Vec<u16> = ctx.objects.suppressed_list(owner).to_vec();
    let arr = ctx
        .arrays
        .alloc(crate::array_heap::ATYPE_REF, list.len() as u16)
        .ok_or(JvmError::StackOverflow)?;
    for (i, &t) in list.iter().enumerate() {
        // ObjectRefs are stored untagged in ATYPE_REF arrays (see the GC's
        // tag scheme); aaload turns them back into Value::ObjectRef.
        ctx.arrays.store(arr, i, t as i32);
    }
    Ok(Some(Value::ArrayRef(arr)))
}

/// `Throwable.getCause()`: the cause recorded in the side table (today only
/// written by the interpreter's ExceptionInInitializerError wrapping), or
/// null — Android/Java's contract for a cause-less throwable.
fn throwable_get_cause(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let Some(Value::ObjectRef(owner)) = ctx.args.first().copied() else {
        return Err(JvmError::InvalidReference);
    };
    Ok(Some(match ctx.objects.get_exception_cause(owner) {
        Some(cause) => Value::ObjectRef(cause),
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
        "addSuppressed" => Some(throwable_add_suppressed(ctx)),
        "getSuppressed" => Some(throwable_get_suppressed(ctx)),
        "getCause" => Some(throwable_get_cause(ctx)),
        _ => None,
    }
}

/// `java/lang/Object` dispatcher: `<init>`, `clone`, and the identity
/// `equals`/`hashCode`/`toString` every object inherits. These are reached
/// only when nothing more specific claimed the call — a Java override on the
/// receiver's class chain resolves to a Java frame before native dispatch,
/// and the builtin dispatchers (String, boxed, Enum, StringBuilder, …) sit
/// before `Object` on the `builtin_super` walk — so a data class's own
/// `equals` still wins and a plain `new Object()` behaves as in Java.
///
/// Identity `hashCode` is the object's heap slot index (reused after GC —
/// stable for the object's lifetime, not unique over time), and identity
/// `toString` is `<class>@<hex slot>`; both accept arrays too. A string
/// Reference receiver normally dispatches straight to the String
/// dispatcher; the `toString` arm keeps returning it unchanged for the
/// `<clinit>`-time and handler-originated calls that still name `Object`.
fn dispatch_object(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            capture_throwable_message(ctx);
            Some(Ok(None))
        }
        "toString" => match ctx.args.first().copied() {
            Some(Value::Reference(idx)) => Some(Ok(Some(Value::Reference(idx)))),
            Some(v @ (Value::ObjectRef(_) | Value::ArrayRef(_))) => {
                Some(identity_to_string(ctx, v))
            }
            _ => None,
        },
        "equals" => match (ctx.args.first().copied(), ctx.args.get(1).copied()) {
            (Some(a @ (Value::ObjectRef(_) | Value::ArrayRef(_))), Some(b)) => {
                Some(Ok(Some(Value::Int((a == b) as i32))))
            }
            _ => None,
        },
        "hashCode" => match ctx.args.first().copied() {
            Some(Value::ObjectRef(idx) | Value::ArrayRef(idx)) => {
                Some(Ok(Some(Value::Int(idx as i32))))
            }
            _ => None,
        },
        // Object.clone(): shallow copy per the Java spec — field slots are
        // copied verbatim, so reference fields share their referents. The
        // fresh object is returned straight onto the caller's operand stack
        // (stack-rooted; GC only runs between opcodes), so no extra rooting
        // is needed. Documented divergence: the Cloneable marker is NOT
        // checked — native dispatch has no view of the interface table — so
        // clone() on a non-Cloneable succeeds instead of throwing
        // CloneNotSupportedException (consistent with the unchecked array
        // clone above).
        "clone" => match ctx.args.first() {
            Some(Value::ObjectRef(idx)) => Some(
                ctx.objects
                    .clone_object(*idx)
                    .map(|new_idx| Some(Value::ObjectRef(new_idx)))
                    .ok_or(JvmError::InvalidReference),
            ),
            _ => None,
        },
        _ => None,
    }
}

/// Java's `Object.toString()` default: `<dotted class name>@<identity hash
/// as four hex digits>` (arrays print as `[I@…` / `[Ljava.lang.Object;@…`).
fn identity_to_string(ctx: &mut NativeContext<'_>, v: Value) -> Result<Option<Value>, JvmError> {
    let (name, idx): (&str, u16) = match v {
        Value::ObjectRef(idx) => (ctx.objects.class_name(idx).unwrap_or("?"), idx),
        Value::ArrayRef(idx) => (
            crate::interpreter::array_class_name(
                ctx.arrays
                    .atype(idx)
                    .unwrap_or(crate::array_heap::ATYPE_REF),
            ),
            idx,
        ),
        _ => return Err(JvmError::InvalidReference),
    };
    // Fixed buffer, no Vec growth paths; a class name longer than the
    // buffer is truncated (the identity suffix is what matters).
    let mut buf = [0u8; 80];
    let mut n = 0;
    for b in name.bytes().take(buf.len() - 5) {
        buf[n] = if b == b'/' { b'.' } else { b };
        n += 1;
    }
    buf[n] = b'@';
    n += 1;
    for shift in [12u32, 8, 4, 0] {
        buf[n] = b"0123456789abcdef"[((idx >> shift) & 0xF) as usize];
        n += 1;
    }
    let s = ctx
        .strings
        .intern_dyn(&buf[..n])
        .ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::Reference(s)))
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
        // addSuppressed stores for real (try-with-resources emits these
        // calls when close() throws); getSuppressed returns the recorded
        // array. Java contract honored: addSuppressed(null) throws NPE,
        // addSuppressed(this) throws IllegalArgumentException.
        "addSuppressed" => Some(throwable_add_suppressed(ctx)),
        "getSuppressed" => Some(throwable_get_suppressed(ctx)),
        "getCause" => Some(throwable_get_cause(ctx)),
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
    /// Loaded class files.  Lets a handler canonicalize a class name to the
    /// class file's genuinely-`'static` (Flash-backed) name via
    /// [`NativeContext::canonical_class_name`] — required before storing a name
    /// past the current call, since a `&str` from [`StringTable::resolve`] may
    /// point into the GC-managed dynamic-string region.
    pub classes: &'a [ClassFile],
}

impl NativeContext<'_> {
    /// Resolve `name` to the loaded class file's genuinely-`'static`
    /// (Flash-backed) class name, or `None` if no loaded class matches.
    ///
    /// Handlers that persist a class name beyond the current native call (into
    /// `class_table`, a service registry, a pending op, …) must route it
    /// through here rather than transmuting a [`StringTable::resolve`] result to
    /// `&'static`: an Intent target-class name is commonly a runtime dynamic
    /// String (e.g. `Class.getName().replace('.', '/')`) whose backing `Vec` the
    /// GC can free, leaving any retained pointer dangling.
    pub fn canonical_class_name(&self, name: &str) -> Option<&'static str> {
        for cf in self.classes {
            if let Some(n) = cf.class_name() {
                if n == name.as_bytes() {
                    return core::str::from_utf8(n).ok();
                }
            }
        }
        None
    }
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
/// | `java/lang/Object` | `<init>`, `clone`, identity `equals`/`hashCode`/`toString` (any object or array; reached only when no Java override and no more specific builtin claims the call) |
/// | `java/lang/Throwable` | `<init>`, `addSuppressed` |
/// | `java/lang/Exception` | `<init>` |
/// | `java/lang/RuntimeException` | `<init>` |
/// | `java/lang/StringBuilder` | `<init>`, `<init>(String)`, `append(String/int/char/long/float/double/boolean/Object)`, `length`, `charAt`, `toString` — `append(Object)` and `String.valueOf(Object)` receive the argument's `toString()` (the interpreter runs a Java override first, else the builtin/identity one) |
/// | `java/lang/String` | `<init>(byte[])`, `<init>(byte[],int,int)`, `length`, `charAt`, `equals`, `equalsIgnoreCase`, `startsWith`, `endsWith`, `contains`, `indexOf`, `lastIndexOf`, `isEmpty`, `compareTo`, `substring`, `trim`, `toUpperCase`, `toLowerCase`, `valueOf`, `concat`, `hashCode`, `toCharArray`, `getBytes`, `format`, `replace`, `split` |
/// | `java/lang/Integer`, `Long`, `Float`, `Double`, `Short`, `Byte` | `<init>`, `valueOf`, `parseX`, `toString`, the `xxxValue()` accessors (unconverted — see the compatibility matrix); `equals` (same class and bits), `hashCode()`/`hashCode(x)`, `compareTo`/`compare` (Java's float total order); `Float.floatToIntBits` |
/// | `java/lang/Boolean` | `<init>`, `valueOf`, `parseBoolean`, `booleanValue`, `toString`, `equals`, `hashCode` (1231/1237), `compare` |
/// | `java/lang/Character` | `<init>`, `valueOf`, `charValue`, `toString`, `equals`, `hashCode`, `compare`; ASCII `isDigit`/`isLetter`/`toUpperCase`/`toLowerCase` |
/// | `java/util/ArrayList` | `<init>`, `add`, `get`, `size`, `isEmpty`, `set`, `remove`, `clear`, `contains`, `iterator`, `toArray` (always a fresh `Object[]`) |
/// | `java/util/HashMap` (alias `LinkedHashMap`, hash-ordered) | `<init>`, `put`, `get`, `remove`, `containsKey`, `containsValue`, `size`, `isEmpty`, `clear`, `getOrDefault`, `keySet`, `values`, `entrySet` — the views answer `iterator`/`size` (key and value views also `contains`); `Map$Entry` answers `getKey`/`getValue` |
/// | `java/util/HashSet` (alias `LinkedHashSet`, hash-ordered) | `<init>`, `add`, `remove`, `contains`, `size`, `isEmpty`, `clear`, `iterator` (the map key-view iterator) |
/// | `java/util/Iterator` | `hasNext`, `next` |
/// | `java/util/Random` | `<init>`, `<init>(long)`, `setSeed`, `nextInt`, `nextInt(int)`, `nextLong`, `nextBoolean`, `nextFloat`, `nextDouble`, `nextGaussian`, `nextBytes` |
/// | `java/util/Arrays` | `sort`, `fill`, `copyOf`, `toString` (all numeric primitive overloads: int/long/double/float/short/byte/char) |
/// | `java/lang/Enum` | `<init>`, `name`, `ordinal`, `toString`, `equals`, `hashCode` (ordinal), `compareTo` — no `valueOf(Class, String)` (see the compatibility matrix) |
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
