// SPDX-License-Identifier: GPL-3.0-only
use crate::names::{c, d, m};
use crate::{
    array_heap::ArrayHeap,
    class_file::ClassFile,
    heap::StringTable,
    object_heap::ObjectHeap,
    static_fields::StaticFieldStore,
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
    c::java_lang_Object,
    c::java_lang_Class,
    c::java_lang_Throwable,
    c::java_lang_Exception,
    c::java_lang_RuntimeException,
    c::java_util_IllegalFormatException,
    c::java_lang_Enum,
    c::java_lang_StringBuilder,
    c::java_lang_String,
    c::java_lang_Integer,
    c::java_lang_Boolean,
    c::java_lang_Long,
    c::java_lang_Float,
    c::java_lang_Double,
    c::java_lang_Character,
    c::java_lang_Byte,
    c::java_lang_Short,
    c::java_util_ArrayList,
    c::java_util_HashMap,
    c::java_util_HashMap_KeySet,
    c::java_util_HashMap_Values,
    c::java_util_HashMap_EntrySet,
    c::java_util_Map_Entry,
    c::java_util_HashSet,
    c::java_util_LinkedHashMap,
    c::java_util_LinkedHashSet,
    c::java_util_Iterator,
    c::java_util_Random,
    c::java_util_Arrays,
    c::java_lang_Math,
    // Canonicalisation-only — handled by the user's NativeMethodHandler, or
    // named in user code as an interface, superclass, lambda SAM, or
    // `instanceof`/`checkcast` target.
    //
    // The `java/**` interfaces here deliberately have no SDK `.java` file.
    // Apps compile with `javac --release 8` and no bootclasspath override, so
    // `java.*` resolves from the JDK's `ct.sym` and an SDK file would be
    // shadowed — it could not document or restrict anything. At run time
    // dispatch goes by the receiver's runtime class and `instanceof` walks
    // `BUILTIN_INTERFACES`, so no class file is ever read for them either. A
    // row here (plus a `BUILTIN_INTERFACES` row for superinterface edges) is
    // the whole cost; `no_bodiless_java_framework_classes` in picodroid-core
    // keeps it that way.
    c::java_lang_System,
    c::java_lang_Runnable,
    c::java_util_Collections,
    c::java_util_List,
    c::java_lang_Comparable,
    c::java_util_Comparator,
    c::java_lang_Cloneable,
    // A legal lambda SAM (`AutoCloseable c = () -> ...`): without a row the
    // proxy's interface canonicalises to "unknown" and `instanceof` fails.
    c::java_lang_AutoCloseable,
    // Classfile-less classes that user code may `new`, `checkcast` or
    // `instanceof` (every name in the interpreter's `BUILTIN_SUPER` /
    // `BUILTIN_INTERFACES` tables). A `new` of a name missing here yields
    // an `"unknown"`-class object that no catch clause ever matches.
    c::java_lang_Number,
    c::java_lang_CharSequence,
    c::java_lang_Appendable,
    c::java_lang_Iterable,
    c::java_util_Collection,
    c::java_util_Set,
    c::java_util_Map,
    c::java_lang_Error,
    c::java_lang_IllegalArgumentException,
    c::java_lang_IllegalStateException,
    c::java_lang_NullPointerException,
    c::java_lang_ArithmeticException,
    c::java_lang_ClassCastException,
    c::java_lang_UnsupportedOperationException,
    c::java_lang_IndexOutOfBoundsException,
    c::java_lang_ArrayIndexOutOfBoundsException,
    c::java_lang_ArrayStoreException,
    c::java_lang_StringIndexOutOfBoundsException,
    c::java_lang_NumberFormatException,
    c::java_lang_ExceptionInInitializerError,
    c::java_lang_StackOverflowError,
    c::java_lang_NegativeArraySizeException,
    c::java_util_ConcurrentModificationException,
    c::java_lang_OutOfMemoryError,
    c::java_lang_InterruptedException,
    c::java_lang_IllegalThreadStateException,
    c::java_lang_IllegalMonitorStateException,
    c::java_util_concurrent_ExecutionException,
    c::java_util_concurrent_CancellationException,
    c::java_util_concurrent_TimeoutException,
    c::java_util_concurrent_RejectedExecutionException,
    c::java_util_NoSuchElementException,
    c::java_io_IOException,
    c::java_io_InterruptedIOException,
    c::java_net_SocketTimeoutException,
    c::java_net_SocketException,
    c::java_net_ConnectException,
    c::java_net_NoRouteToHostException,
    c::java_net_BindException,
    c::java_net_UnknownHostException,
    c::java_net_ProtocolException,
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
    (c::java_lang_Class, m::getName, d::__String),
    // java/lang/Math
    (c::java_lang_Math, m::abs, "(D)D"),
    (c::java_lang_Math, m::abs, "(F)F"),
    (c::java_lang_Math, m::abs, "(I)I"),
    (c::java_lang_Math, m::abs, "(J)J"),
    (c::java_lang_Math, m::atan2, "(DD)D"),
    (c::java_lang_Math, m::ceil, "(D)D"),
    (c::java_lang_Math, m::cos, "(D)D"),
    (c::java_lang_Math, m::exp, "(D)D"),
    (c::java_lang_Math, m::floor, "(D)D"),
    (c::java_lang_Math, m::log, "(D)D"),
    (c::java_lang_Math, m::log10, "(D)D"),
    (c::java_lang_Math, m::max, "(DD)D"),
    (c::java_lang_Math, m::max, "(FF)F"),
    (c::java_lang_Math, m::max, "(II)I"),
    (c::java_lang_Math, m::max, "(JJ)J"),
    (c::java_lang_Math, m::min, "(DD)D"),
    (c::java_lang_Math, m::min, "(FF)F"),
    (c::java_lang_Math, m::min, "(II)I"),
    (c::java_lang_Math, m::min, "(JJ)J"),
    (c::java_lang_Math, m::pow, "(DD)D"),
    (c::java_lang_Math, m::round, "(D)J"),
    (c::java_lang_Math, m::round, "(F)I"),
    (c::java_lang_Math, m::sin, "(D)D"),
    (c::java_lang_Math, m::sqrt, "(D)D"),
    (c::java_lang_Math, m::tan, "(D)D"),
    (c::java_lang_Math, m::toDegrees, "(D)D"),
    (c::java_lang_Math, m::toRadians, "(D)D"),
    // java/lang/System
    (c::java_lang_System, m::arraycopy, d::Object_I_Object_I_I__V),
    // java/util/Arrays
    (c::java_util_Arrays, m::copyOf, "([BI)[B"),
    (c::java_util_Arrays, m::copyOf, "([CI)[C"),
    (c::java_util_Arrays, m::copyOf, "([DI)[D"),
    (c::java_util_Arrays, m::copyOf, "([FI)[F"),
    (c::java_util_Arrays, m::copyOf, "([II)[I"),
    (c::java_util_Arrays, m::copyOf, "([JI)[J"),
    (c::java_util_Arrays, m::copyOf, "([SI)[S"),
    (c::java_util_Arrays, m::fill, "([BB)V"),
    (c::java_util_Arrays, m::fill, "([CC)V"),
    (c::java_util_Arrays, m::fill, "([DD)V"),
    (c::java_util_Arrays, m::fill, "([FF)V"),
    (c::java_util_Arrays, m::fill, "([II)V"),
    (c::java_util_Arrays, m::fill, "([JJ)V"),
    (c::java_util_Arrays, m::fill, "([SS)V"),
    (c::java_util_Arrays, m::sort, "([B)V"),
    (c::java_util_Arrays, m::sort, "([C)V"),
    (c::java_util_Arrays, m::sort, "([D)V"),
    (c::java_util_Arrays, m::sort, "([F)V"),
    (c::java_util_Arrays, m::sort, "([I)V"),
    (c::java_util_Arrays, m::sort, "([J)V"),
    (c::java_util_Arrays, m::sort, "([S)V"),
    (c::java_util_Arrays, m::toString, d::aB__String),
    (c::java_util_Arrays, m::toString, d::aC__String),
    (c::java_util_Arrays, m::toString, d::aD__String),
    (c::java_util_Arrays, m::toString, d::aF__String),
    (c::java_util_Arrays, m::toString, d::aI__String),
    (c::java_util_Arrays, m::toString, d::aJ__String),
    (c::java_util_Arrays, m::toString, d::aS__String),
];

/// One served member of a builtin class: `(name, descriptors)`.
///
/// Native dispatch is keyed on `(class, method name)`, so most arms serve
/// every overload javac can resolve; those rows carry an empty descriptor
/// list ("any descriptor"). A non-empty list names the only descriptors the
/// arm serves *correctly* — any other overload reaches a wider arm and
/// mis-serves silently: `new String(char[])` falls through to
/// `Object.<init>` and yields a non-string, `Integer.parseInt(s, radix)`
/// ignores the radix, `new ArrayList<>(other)` ignores the source,
/// `StringBuilder.append(char[])` appends nothing.
pub type BuiltinMethodRow = (&'static str, &'static [&'static str]);

/// Every method the builtin dispatchers serve, per class — the
/// machine-readable twin of the rustdoc table on [`BuiltinHandler`] and one
/// input to the generated compile-time contract (`sdk/api-contract.tsv`,
/// written by picodroid-core's `api_contract` test). Keys are exactly the
/// classes of `BUILTIN_DISPATCH` (`builtin_methods_cover_every_dispatch_class`)
/// and every name must be a literal in its dispatcher's source
/// (`builtin_method_rows_name_real_arms`). The reverse — an arm with no row —
/// is not machine-checked; it surfaces as a contract failure on the first
/// app that uses the arm, naming the row to add here.
///
/// Members served outside these dispatchers are listed with their site:
/// `Object.getClass` and `ArrayList.sort` resolve in
/// `interpreter/ops_invoke.rs` before dispatch; `Object.wait/notify/notifyAll`
/// are the embedder's (picodroid-core `PLATFORM_BUILTIN_METHODS`).
pub const BUILTIN_METHODS: &[(&str, &[BuiltinMethodRow])] = &[
    (c::java_lang_Object, OBJECT_METHODS),
    (c::java_lang_Class, CLASS_METHODS),
    (c::java_lang_Throwable, THROWABLE_METHODS),
    (c::java_lang_Exception, THROWABLE_METHODS),
    (c::java_lang_RuntimeException, THROWABLE_METHODS),
    (c::java_util_IllegalFormatException, THROWABLE_METHODS),
    (c::java_lang_Enum, ENUM_METHODS),
    (c::java_lang_StringBuilder, STRING_BUILDER_METHODS),
    (c::java_lang_String, STRING_METHODS),
    (c::java_lang_Integer, INTEGER_METHODS),
    (c::java_lang_Boolean, BOOLEAN_METHODS),
    (c::java_lang_Long, LONG_METHODS),
    (c::java_lang_Float, FLOAT_METHODS),
    (c::java_lang_Double, DOUBLE_METHODS),
    (c::java_lang_Character, CHARACTER_METHODS),
    (c::java_lang_Byte, BYTE_METHODS),
    (c::java_lang_Short, SHORT_METHODS),
    (c::java_util_ArrayList, ARRAY_LIST_METHODS),
    (c::java_util_HashMap, HASH_MAP_METHODS),
    (c::java_util_HashMap_KeySet, HASH_MAP_VIEW_METHODS),
    (c::java_util_HashMap_Values, HASH_MAP_VIEW_METHODS),
    (c::java_util_HashMap_EntrySet, HASH_MAP_VIEW_METHODS),
    (c::java_util_Map_Entry, MAP_ENTRY_METHODS),
    (c::java_util_HashSet, HASH_SET_METHODS),
    (c::java_util_LinkedHashMap, HASH_MAP_METHODS),
    (c::java_util_LinkedHashSet, HASH_SET_METHODS),
    (c::java_util_Iterator, ITERATOR_METHODS),
    (c::java_util_Random, RANDOM_METHODS),
    (c::java_util_Arrays, ARRAYS_METHODS),
    (c::java_lang_Math, MATH_METHODS),
    (c::java_lang_System, SYSTEM_METHODS),
];

const OBJECT_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &[]),
    (m::toString, &[]),
    (m::equals, &[]),
    (m::hashCode, &[]),
    (m::clone, &[]),
    // Answered by the interpreter (needs the class-object cache), not an arm.
    (m::getClass, &[d::__Class]),
];

const CLASS_METHODS: &[BuiltinMethodRow] = &[("<init>", &[]), (m::getName, &[])];

/// `dispatch_throwable` and `dispatch_init_only` serve the same names.
const THROWABLE_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &[]),
    (m::getMessage, &[]),
    (m::addSuppressed, &[]),
    (m::getSuppressed, &[]),
    (m::getCause, &[]),
];

const ENUM_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &[]),
    (m::name, &[]),
    (m::toString, &[]),
    (m::ordinal, &[]),
    (m::equals, &[]),
    (m::hashCode, &[]),
    (m::compareTo, &[]),
];

const STRING_BUILDER_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &[]),
    // The arm switches on the argument's Value variant; an array or a
    // non-String CharSequence argument appends nothing, hence the exact
    // list. A call through `Appendable` carries that return type instead.
    (
        m::append,
        &[
            d::String__StringBuilder,
            d::CharSequence__StringBuilder,
            d::C__Appendable,
            d::CharSequence__Appendable,
            d::Object__StringBuilder,
            d::C__StringBuilder,
            d::Z__StringBuilder,
            d::I__StringBuilder,
            d::J__StringBuilder,
            d::F__StringBuilder,
            d::D__StringBuilder,
        ],
    ),
    (m::length, &[]),
    (m::charAt, &[]),
    (m::toString, &[]),
];

const STRING_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &["([B)V", "([BII)V"]),
    (m::format, &[d::String_aObject__String]),
    (
        m::valueOf,
        &[
            d::Object__String,
            d::Z__String,
            d::C__String,
            d::I__String,
            d::J__String,
            d::F__String,
            d::D__String,
        ],
    ),
    (m::length, &[]),
    (m::charAt, &[]),
    (m::isEmpty, &[]),
    (m::equals, &[]),
    (m::equalsIgnoreCase, &[]),
    (m::startsWith, &[]),
    (m::endsWith, &[]),
    (m::contains, &[]),
    (m::indexOf, &[]),
    (m::lastIndexOf, &[]),
    (m::compareTo, &[]),
    (m::substring, &[]),
    (m::trim, &[]),
    (m::toUpperCase, &[]),
    (m::toLowerCase, &[]),
    (m::concat, &[]),
    (m::hashCode, &[]),
    (m::toString, &[]),
    (m::toCharArray, &[]),
    (m::getBytes, &[]),
    (m::replace, &[]),
    (m::split, &[]),
];

/// The numeric wrappers share `boxed_dispatch!` + `dispatch_common` plus
/// per-class parse / `toString` / `valueOf` arms: `$p` is the primitive
/// descriptor, `$b` the boxed class, `$parse` the parse method. The radix
/// overloads of `parseX` / `toString` / `valueOf` are not served (the
/// radix argument is ignored), hence the exact lists.
macro_rules! numeric_box_methods {
    ($box_from_prim:expr, $box_from_string:expr, $parse:expr, $parse_desc:expr, $prim_to_string:expr) => {
        &[
            ("<init>", &[]),
            (m::valueOf, &[$box_from_prim, $box_from_string]),
            ($parse, &[$parse_desc]),
            (m::toString, &[d::__String, $prim_to_string]),
            (m::intValue, &[]),
            (m::longValue, &[]),
            (m::floatValue, &[]),
            (m::doubleValue, &[]),
            (m::shortValue, &[]),
            (m::byteValue, &[]),
            (m::equals, &[]),
            (m::hashCode, &[]),
            (m::compareTo, &[]),
            (m::compare, &[]),
        ]
    };
}

const INTEGER_METHODS: &[BuiltinMethodRow] = numeric_box_methods!(
    d::I__Integer,
    d::String__Integer,
    m::parseInt,
    d::String__I,
    d::I__String
);
const LONG_METHODS: &[BuiltinMethodRow] = numeric_box_methods!(
    d::J__Long,
    d::String__Long,
    m::parseLong,
    d::String__J,
    d::J__String
);
const DOUBLE_METHODS: &[BuiltinMethodRow] = numeric_box_methods!(
    d::D__Double,
    d::String__Double,
    m::parseDouble,
    d::String__D,
    d::D__String
);
const BYTE_METHODS: &[BuiltinMethodRow] = numeric_box_methods!(
    d::B__Byte,
    d::String__Byte,
    m::parseByte,
    d::String__B,
    d::B__String
);
const SHORT_METHODS: &[BuiltinMethodRow] = numeric_box_methods!(
    d::S__Short,
    d::String__Short,
    m::parseShort,
    d::String__S,
    d::S__String
);
const FLOAT_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &[]),
    (m::valueOf, &[d::F__Float, d::String__Float]),
    (m::parseFloat, &[d::String__F]),
    (m::toString, &[d::__String, d::F__String]),
    (m::intValue, &[]),
    (m::longValue, &[]),
    (m::floatValue, &[]),
    (m::doubleValue, &[]),
    (m::shortValue, &[]),
    (m::byteValue, &[]),
    (m::equals, &[]),
    (m::hashCode, &[]),
    (m::compareTo, &[]),
    (m::compare, &[]),
    (m::floatToIntBits, &[]),
];

const BOOLEAN_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &[]),
    (m::valueOf, &[]),
    (m::parseBoolean, &[]),
    (m::booleanValue, &[]),
    (m::toString, &[]),
    (m::equals, &[]),
    (m::hashCode, &[]),
    (m::compareTo, &[]),
    (m::compare, &[]),
];

const CHARACTER_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &[]),
    (m::valueOf, &[]),
    (m::charValue, &[]),
    (m::toString, &[]),
    (m::equals, &[]),
    (m::hashCode, &[]),
    (m::compareTo, &[]),
    (m::compare, &[]),
    (m::isDigit, &[]),
    (m::isLetter, &[]),
    (m::toUpperCase, &[]),
    (m::toLowerCase, &[]),
];

const ARRAY_LIST_METHODS: &[BuiltinMethodRow] = &[
    // The copy constructor `<init>(Collection)` ignores its argument.
    ("<init>", &["()V", "(I)V"]),
    (m::add, &[]),
    (m::get, &[]),
    (m::size, &[]),
    (m::isEmpty, &[]),
    (m::set, &[]),
    (m::remove, &[]),
    (m::clear, &[]),
    (m::iterator, &[]),
    (m::toArray, &[]),
    (m::contains, &[]),
    // Resolved by the interpreter: the comparator is a Java upcall.
    (m::sort, &[d::Comparator__V]),
];

const HASH_MAP_METHODS: &[BuiltinMethodRow] = &[
    // The copy constructor `<init>(Map)` ignores its argument.
    ("<init>", &["()V", "(I)V", "(IF)V"]),
    (m::put, &[]),
    (m::get, &[]),
    (m::remove, &[]),
    (m::containsKey, &[]),
    (m::containsValue, &[]),
    (m::size, &[]),
    (m::isEmpty, &[]),
    (m::clear, &[]),
    (m::getOrDefault, &[]),
    (m::keySet, &[]),
    (m::values, &[]),
    (m::entrySet, &[]),
];

const HASH_MAP_VIEW_METHODS: &[BuiltinMethodRow] =
    &[(m::iterator, &[]), (m::size, &[]), (m::contains, &[])];

const MAP_ENTRY_METHODS: &[BuiltinMethodRow] = &[(m::getKey, &[]), (m::getValue, &[])];

const HASH_SET_METHODS: &[BuiltinMethodRow] = &[
    // The copy constructor `<init>(Collection)` ignores its argument.
    ("<init>", &["()V", "(I)V", "(IF)V"]),
    (m::add, &[]),
    (m::remove, &[]),
    (m::contains, &[]),
    (m::size, &[]),
    (m::isEmpty, &[]),
    (m::iterator, &[]),
    (m::clear, &[]),
];

const ITERATOR_METHODS: &[BuiltinMethodRow] =
    &[(m::hasNext, &[]), (m::next, &[]), (m::remove, &[])];

const RANDOM_METHODS: &[BuiltinMethodRow] = &[
    ("<init>", &[]),
    (m::setSeed, &[]),
    (m::nextInt, &[]),
    (m::nextLong, &[]),
    (m::nextBoolean, &[]),
    (m::nextFloat, &[]),
    (m::nextDouble, &[]),
    (m::nextGaussian, &[]),
    (m::nextBytes, &[]),
];

/// Name-level on purpose: the arms are `atype`-driven and serve the range
/// (`sort(a, from, to)`), `boolean[]` and `Object[]` overloads that the SDK's
/// `Arrays.java` never declares (a call to an undeclared method on a loaded
/// class falls through to native dispatch).
const ARRAYS_METHODS: &[BuiltinMethodRow] = &[
    (m::sort, &[]),
    (m::fill, &[]),
    (m::copyOf, &[]),
    (m::toString, &[]),
];

const MATH_METHODS: &[BuiltinMethodRow] = &[
    (m::abs, &[]),
    (m::min, &[]),
    (m::max, &[]),
    (m::sqrt, &[]),
    (m::pow, &[]),
    (m::floor, &[]),
    (m::ceil, &[]),
    (m::round, &[]),
    (m::sin, &[]),
    (m::cos, &[]),
    (m::tan, &[]),
    (m::atan2, &[]),
    (m::toRadians, &[]),
    (m::toDegrees, &[]),
    (m::log, &[]),
    (m::log10, &[]),
    (m::exp, &[]),
];

const SYSTEM_METHODS: &[BuiltinMethodRow] = &[(m::arraycopy, &[])];

/// Abstract members of the classfile-less `java/**` interfaces that resolve
/// on whatever implements them — a lambda proxy (`try_lambda_dispatch` runs
/// the body for any call on the proxy), an app class (its own bytecode) or a
/// builtin (its `BUILTIN_METHODS` rows). The compile-time contract resolves
/// interfaces that builtins implement (`List`, `Map`, `CharSequence`, …)
/// through `BUILTIN_INTERFACES`, so this is only the SAM set apps target
/// with lambdas and that no builtin implements. Every key must be a
/// `BUILTIN_CLASS_NAMES` name and not a dispatched class.
pub const BUILTIN_INTERFACE_METHODS: &[(&str, &[BuiltinMethodRow])] = &[
    (c::java_lang_Runnable, &[(m::run, &["()V"])]),
    (
        c::java_util_Comparator,
        &[(m::compare, &[d::Object_Object__I])],
    ),
    (c::java_lang_Comparable, &[(m::compareTo, &[d::Object__I])]),
    (c::java_lang_AutoCloseable, &[(m::close, &["()V"])]),
    (c::java_lang_Iterable, &[(m::iterator, &[d::__Iterator])]),
];

/// Table consulted by [`BuiltinHandler::dispatch`]. Single source of truth:
/// changing this table changes the dispatch behaviour. The
/// `builtin_dispatch_classes_subset_of_names` test asserts every class here is
/// also in [`BUILTIN_CLASS_NAMES`] so canonicalisation cannot drift.
const BUILTIN_DISPATCH: &[(&str, BuiltinDispatchFn)] = &[
    (c::java_lang_Object, dispatch_object),
    (c::java_lang_Class, class_obj::dispatch),
    (c::java_lang_Throwable, dispatch_throwable),
    (c::java_lang_Exception, dispatch_init_only),
    (c::java_lang_RuntimeException, dispatch_init_only),
    (c::java_util_IllegalFormatException, dispatch_init_only),
    (c::java_lang_Enum, enumeration::dispatch),
    (c::java_lang_StringBuilder, string_builder::dispatch),
    (c::java_lang_String, string::dispatch),
    (c::java_lang_Integer, boxed::dispatch_integer),
    (c::java_lang_Boolean, boxed::dispatch_boolean),
    (c::java_lang_Long, boxed::dispatch_long),
    (c::java_lang_Float, boxed::dispatch_float),
    (c::java_lang_Double, boxed::dispatch_double),
    (c::java_lang_Character, boxed::dispatch_character),
    (c::java_lang_Byte, boxed::dispatch_byte),
    (c::java_lang_Short, boxed::dispatch_short),
    (c::java_util_ArrayList, collections::dispatch),
    (c::java_util_HashMap, hashmap::dispatch),
    (c::java_util_HashMap_KeySet, hashmap::dispatch_view),
    (c::java_util_HashMap_Values, hashmap::dispatch_view),
    (c::java_util_HashMap_EntrySet, hashmap::dispatch_view),
    (c::java_util_Map_Entry, hashmap::dispatch_entry),
    (c::java_util_HashSet, hashset::dispatch),
    // Insertion-ordered aliases (documented divergence: hash order). The
    // no-arg `mutableMapOf()`/`mutableSetOf()` are inline in Kotlin and emit
    // `new java/util/LinkedHashMap` at the call site.
    (c::java_util_LinkedHashMap, hashmap::dispatch),
    (c::java_util_LinkedHashSet, hashset::dispatch),
    (c::java_util_Iterator, iterator::dispatch),
    (c::java_util_Random, random::dispatch),
    (c::java_util_Arrays, arrays::dispatch),
    (c::java_lang_Math, math::dispatch),
    // System is otherwise canonicalisation-only (currentTimeMillis lives in
    // the platform handler, which dispatches first); arraycopy is pure array
    // machinery, so it belongs to the builtins.
    (c::java_lang_System, arrays::dispatch_system),
];

/// If the receiver is a Throwable being constructed with a String and/or a
/// Throwable argument (`<init>(Ljava/lang/String;)V`,
/// `<init>(Ljava/lang/Throwable;)V`,
/// `<init>(Ljava/lang/String;Ljava/lang/Throwable;)V`), record the message
/// and the cause in the ObjectHeap side tables so `getMessage()` /
/// `getCause()` and `UncaughtException` can surface them. The cause-only
/// form leaves the message unset where Java would set it to
/// `cause.toString()` — a documented shortcut.
fn capture_throwable_message(ctx: &mut NativeContext<'_>) {
    let Some(Value::ObjectRef(obj_idx)) = ctx.args.first().copied() else {
        return;
    };
    let desc = ctx.descriptor;
    if desc.starts_with(crate::names::d::p_String) {
        if let Some(Value::Reference(msg_idx)) = ctx.args.get(1).copied() {
            ctx.objects.register_exception_message(obj_idx, msg_idx);
        }
        if desc.starts_with(crate::names::d::p_String_Throwable) {
            if let Some(Value::ObjectRef(cause)) = ctx.args.get(2).copied() {
                ctx.objects.register_exception_cause(obj_idx, cause);
            }
        }
    } else if desc.starts_with(crate::names::d::p_Throwable) {
        if let Some(Value::ObjectRef(cause)) = ctx.args.get(1).copied() {
            ctx.objects.register_exception_cause(obj_idx, cause);
        }
    }
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
            Err(throw_named(ctx, c::java_lang_IllegalArgumentException))
        }
        Some(Value::ObjectRef(t)) => {
            ctx.objects.add_suppressed(owner, t);
            Ok(None)
        }
        Some(Value::Null) | None => Err(throw_named(ctx, c::java_lang_NullPointerException)),
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
        // Same slot encoding aastore uses; aaload decodes it back.
        let raw = crate::array_heap::encode_ref(Value::ObjectRef(t)).unwrap_or(0);
        ctx.arrays.store(arr, i, raw);
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
        m::getMessage => Some(throwable_get_message(ctx)),
        m::addSuppressed => Some(throwable_add_suppressed(ctx)),
        m::getSuppressed => Some(throwable_get_suppressed(ctx)),
        m::getCause => Some(throwable_get_cause(ctx)),
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
        m::toString => match ctx.args.first().copied() {
            Some(Value::Reference(idx)) => Some(Ok(Some(Value::Reference(idx)))),
            Some(v @ (Value::ObjectRef(_) | Value::ArrayRef(_))) => {
                Some(identity_to_string(ctx, v))
            }
            _ => None,
        },
        m::equals => match (ctx.args.first().copied(), ctx.args.get(1).copied()) {
            (Some(a @ (Value::ObjectRef(_) | Value::ArrayRef(_))), Some(b)) => {
                Some(Ok(Some(Value::Int((a == b) as i32))))
            }
            _ => None,
        },
        m::hashCode => match ctx.args.first().copied() {
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
        m::clone => match ctx.args.first() {
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
        m::getMessage => Some(throwable_get_message(ctx)),
        // addSuppressed stores for real (try-with-resources emits these
        // calls when close() throws); getSuppressed returns the recorded
        // array. Java contract honored: addSuppressed(null) throws NPE,
        // addSuppressed(this) throws IllegalArgumentException.
        m::addSuppressed => Some(throwable_add_suppressed(ctx)),
        m::getSuppressed => Some(throwable_get_suppressed(ctx)),
        m::getCause => Some(throwable_get_cause(ctx)),
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
    /// The rest of the interpreter state, present whenever this call came from
    /// running bytecode. [`NativeMethodHandler::invoke_java`] needs it; nothing
    /// else does, and its contents are deliberately crate-private.
    ///
    /// `None` when a handler is driven outside the interpreter (unit tests,
    /// direct dispatch), which is why `invoke_java` can fail rather than
    /// assuming it is there.
    pub upcall: Option<&'a mut UpcallEnv<'a>>,
}

/// The interpreter state a synchronous native→Java upcall needs — everything
/// in `Executor` *except* the handler.
///
/// The omission is the whole design. While `dispatch` runs, the arm holds
/// `&mut H` exclusively; the nested executor gets its handler by reborrowing
/// that same `&mut H` (the arm lends itself via `self.invoke_java(..)`), and
/// takes everything else from here. Carrying the handler in this struct
/// instead would hand the nested executor a *second* `&mut H` — the aliasing
/// bug that sank the original "park a `*mut Executor` in a static cell"
/// sketch.
///
/// Fields are crate-private: arms pass the whole `NativeContext` back to
/// [`NativeMethodHandler::invoke_java`] and never reach in here themselves.
pub struct UpcallEnv<'a> {
    pub(crate) statics: &'a mut StaticFieldStore,
    pub(crate) gc_state: &'a mut crate::gc::GcState,
    pub(crate) class_objects: &'a mut crate::class_objects::ClassObjectCache,
    pub(crate) frames: &'a mut alloc::vec::Vec<crate::frame::Frame>,
    /// Upcall nesting already on this Rust stack, so the nested executor
    /// continues the count instead of restarting it.
    pub(crate) upcall_depth: u8,
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
    /// Synchronously call a Java method from inside a native arm and return
    /// its value — the embedder-facing half of the native→Java upcall.
    ///
    /// `args` excludes `recv`. Both are GC-rooted for the duration; **any
    /// other `Value` the arm holds across this call must be re-read from the
    /// heap afterwards**, because the callee runs arbitrary Java, which
    /// allocates, which collects.
    ///
    /// Taking `&mut self` *and* `&mut NativeContext` is load-bearing, not
    /// stylistic: it makes the borrow checker reject an arm that holds a
    /// `ctx.objects`-derived reference across the call, which would otherwise
    /// dangle when the callee mutates the heap.
    ///
    /// Two obligations this cannot check for you:
    /// - An arm holding side state (a slot-table entry, a half-mutated
    ///   buffer) must not `?` straight out of this call — an `Err` skips
    ///   whatever cleanup follows it.
    /// - Never call it from inside an [`crate::atomic_section`] guard. Those
    ///   suspend the scheduler and must not block; arbitrary Java can do both.
    ///
    /// Fails with [`JvmError::NoSuchMethod`] if this handler was driven from
    /// outside the interpreter (`ctx.upcall` is `None`).
    fn invoke_java(
        &mut self,
        ctx: &mut NativeContext<'_>,
        recv: Value,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError>
    where
        Self: Sized,
    {
        crate::interpreter::upcall_from_native(self, ctx, recv, method_name, descriptor, args)
    }

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

    /// Drop monitor state for entities the collector just freed.
    ///
    /// Called by the interpreter right after every collection, before any
    /// allocation can reuse a freed slot, with `live` answering whether the
    /// entity a key names survived the sweep. A [`MonitorKey`] is a heap slot
    /// index and slots are recycled, so without this a new object landing in
    /// a dead object's slot would inherit its monitor — and every monitor
    /// ever entered would keep its OS mutex forever. Implementations must
    /// keep any monitor that is still held. The default is a no-op.
    fn monitors_prune(&mut self, _live: &dyn Fn(MonitorKey) -> bool) {}

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
/// [`BUILTIN_METHODS`] is the machine-readable form of this table (and the
/// input the compile-time contract is generated from); keep the two in step.
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
        if class_name.starts_with('[') && method_name == m::clone {
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
