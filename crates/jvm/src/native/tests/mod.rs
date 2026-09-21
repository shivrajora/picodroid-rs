// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the builtin native dispatchers, one file per dispatcher they
//! exercise (`string.rs` tests `native/string.rs`, and so on), plus
//! `contract.rs` (the method-table / dispatcher-source cross-checks) and
//! `qa_2026_09_13.rs` (that QA round's regression guards).
//!
//! Every shared harness lives here — `StrCtx`, `SbCtx`, `RngCtx`, the
//! `dispatch_*` helpers, the `S_*` fixtures — so the topic files hold only
//! `#[test]` functions and reach the harness through `use super::*`.

use super::*;
use crate::names::{c, d, m};
use crate::{array_heap::ArrayHeap, heap::StringTable, object_heap::ObjectHeap};

mod arrays;
mod boxed;
mod collections;
mod contract;
mod enumeration;
mod hashmap;
mod iterator;
mod math;
mod qa_2026_09_13;
mod random;
mod string;
mod string_builder;
mod string_format;

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
            classes: &[],
            descriptor: desc,
            args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_lang_String, method, &mut ctx)
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
        classes: &[],
        descriptor,
        args,
        strings: &mut strings,
        objects: &mut objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_lang_Math, method, &mut ctx)
        .expect("Math method not handled")
}

// ── helpers for tests/string.rs ──

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

// ── helpers for tests/string_builder.rs ──

struct SbCtx {
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
    this: u16,
}

impl SbCtx {
    fn new() -> Self {
        let mut objects = ObjectHeap::new();
        let this = objects.alloc(c::java_lang_StringBuilder).unwrap();
        Self {
            strings: StringTable::new(),
            objects,
            arrays: ArrayHeap::new(),
            this,
        }
    }

    fn call(
        &mut self,
        method: &str,
        desc: &str,
        extra: Option<Value>,
    ) -> Result<Option<Value>, JvmError> {
        let this = Value::ObjectRef(self.this);
        let args: alloc::vec::Vec<Value> = match extra {
            None => alloc::vec![this],
            Some(v) => alloc::vec![this, v],
        };
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: desc,
            args: &args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_lang_StringBuilder, method, &mut ctx)
            .expect("StringBuilder method not handled")
    }

    fn to_string(&mut self) -> &str {
        let result = self.call(m::toString, d::__String, None).unwrap().unwrap();
        if let Value::Reference(idx) = result {
            // SAFETY: the string is interned into self.strings and lives as long as self
            let ptr = self.strings.resolve(idx).unwrap_or("") as *const str;
            unsafe { &*ptr }
        } else {
            panic!("toString returned non-Reference")
        }
    }
}

// ── helpers for tests/boxed.rs ──

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
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(class, method, &mut ctx)
        .expect("boxed method not handled")
}

fn dispatch_boxed_to_string(
    class: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(class, m::toString, &mut ctx)
        .expect("toString not handled")
}

fn resolve_str<'a>(strings: &'a StringTable, v: Value) -> &'a str {
    if let Value::Reference(idx) = v {
        strings.resolve(idx).unwrap_or("")
    } else {
        panic!("expected Reference, got {v:?}");
    }
}

// ── helpers for tests/collections.rs ──

fn dispatch_list(
    method: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_ArrayList, method, &mut ctx)
        .expect("ArrayList method not handled")
}

// ── helpers for tests/hashmap.rs ──

fn dispatch_map(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_HashMap, method, &mut ctx)
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
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_HashSet, method, &mut ctx)
        .expect("HashSet method not handled")
}

fn make_map(strings: &mut StringTable, objects: &mut ObjectHeap) -> Value {
    let map = Value::ObjectRef(objects.alloc(c::java_util_HashMap).unwrap());
    dispatch_map("<init>", "()V", &[map], strings, objects).unwrap();
    map
}

fn make_set(strings: &mut StringTable, objects: &mut ObjectHeap) -> Value {
    let set = Value::ObjectRef(objects.alloc(c::java_util_HashSet).unwrap());
    dispatch_set("<init>", "()V", &[set], strings, objects).unwrap();
    set
}

// ── helpers for tests/iterator.rs ──

fn dispatch_iter(
    method: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_Iterator, method, &mut ctx)
        .expect("Iterator method not handled")
}

// ── helpers for tests/enumeration.rs ──

fn dispatch_enum(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_lang_Enum, method, &mut ctx)
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
        d::String_I__V,
        &[obj, name_ref, Value::Int(ordinal)],
        strings,
        objects,
    )
    .unwrap();
    obj
}

// ── helpers for tests/string_format.rs ──

impl StrCtx {
    /// Build an Object[] from a slice of Values, using the REF_TAG encoding
    /// that anewarray/aastore produces in real bytecode.
    fn make_args(&mut self, vals: &[Value]) -> Value {
        let arr = self
            .arrays
            .alloc(crate::array_heap::ATYPE_REF, vals.len() as u16)
            .unwrap();
        for (i, v) in vals.iter().enumerate() {
            let raw = crate::array_heap::encode_ref(*v)
                .expect("make_args only accepts Null / Reference / ObjectRef");
            self.arrays.store(arr, i, raw);
        }
        Value::ArrayRef(arr)
    }

    /// Box a primitive Value into the named wrapper class and return the ObjectRef.
    fn box_primitive(&mut self, class: &'static str, v: Value) -> Value {
        let idx = self.objects.alloc(class).unwrap();
        self.objects.set_field(idx, 0, v);
        Value::ObjectRef(idx)
    }

    /// Convenience: call format("...", new Object[]{...}) and return the &str.
    fn fmt(&mut self, fmt: &'static [u8], args: &[Value]) -> alloc::string::String {
        let fmt_ref = self.intern(fmt);
        let arr = self.make_args(args);
        let result = self
            .dispatch(m::format, d::String_aObject__String, &[fmt_ref, arr])
            .unwrap()
            .unwrap();
        let Value::Reference(idx) = result else {
            panic!("expected Reference, got {result:?}");
        };
        self.strings.resolve(idx).unwrap_or("").into()
    }
}

// ── helpers for tests/random.rs ──

struct RngCtx {
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
    this_idx: u16,
}

impl RngCtx {
    fn new(seed: i64) -> Self {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let this_idx = objects.alloc(c::java_util_Random).unwrap();
        // Seed via the native <init>(J) so behavior matches a real instance.
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: "(J)V",
            args: &[Value::ObjectRef(this_idx), Value::Long(seed)],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_Random, "<init>", &mut ctx)
            .expect("Random.<init>(J) not handled")
            .expect("Random.<init>(J) returned error");
        Self {
            strings,
            objects,
            arrays,
            this_idx,
        }
    }

    fn call(&mut self, method: &str, desc: &str, extra: &[Value]) -> Option<Value> {
        let mut args: alloc::vec::Vec<Value> = alloc::vec![Value::ObjectRef(self.this_idx)];
        args.extend_from_slice(extra);
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: desc,
            args: &args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_Random, method, &mut ctx)
            .expect("Random method not handled")
            .expect("Random method returned error")
    }
}

// ── helpers for tests/arrays.rs ──

fn arrays_dispatch(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_Arrays, method, &mut ctx)
        .expect("Arrays method not handled")
}

fn make_int_array(arrays: &mut ArrayHeap, vs: &[i32]) -> u16 {
    use crate::array_heap::ATYPE_INT;
    let idx = arrays.alloc(ATYPE_INT, vs.len() as u16).unwrap();
    for (i, v) in vs.iter().enumerate() {
        arrays.store(idx, i, *v).unwrap();
    }
    idx
}

fn read_int_array(arrays: &ArrayHeap, idx: u16) -> alloc::vec::Vec<i32> {
    let len = arrays.length(idx).unwrap() as usize;
    (0..len).map(|i| arrays.load(idx, i).unwrap()).collect()
}

fn arrays_to_string_str(
    desc: &str,
    arr: u16,
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
) -> alloc::string::String {
    let v = arrays_dispatch(
        m::toString,
        desc,
        &[Value::ArrayRef(arr)],
        strings,
        objects,
        arrays,
    )
    .unwrap()
    .unwrap();
    let Value::Reference(idx) = v else {
        panic!("expected Reference, got {v:?}");
    };
    strings.resolve(idx).unwrap().into()
}

// ── helpers for tests/boxed.rs ──

fn dispatch_on(
    cx: &mut StrCtx,
    class: &str,
    method: &str,
    desc: &str,
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut cx.strings,
        objects: &mut cx.objects,
        arrays: &mut cx.arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(class, method, &mut ctx)
        .unwrap_or_else(|| panic!("{class}.{method} not handled"))
}

fn boxed(cx: &mut StrCtx, class: &'static str, v: Value) -> Value {
    let idx = cx.objects.alloc(class).unwrap();
    cx.objects.set_field(idx, 0, v);
    Value::ObjectRef(idx)
}

// ── helpers for tests/hashmap.rs ──

const OBJ_DESC: &str = d::__Object;

fn new_map(cx: &mut StrCtx, class: &'static str) -> Value {
    let map = Value::ObjectRef(cx.objects.alloc(class).unwrap());
    dispatch_on(cx, class, "<init>", "()V", &[map]).unwrap();
    map
}

// ── helpers for tests/iterator.rs ──

fn make_list_iterator(objects: &mut ObjectHeap, list: Value) -> Value {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: d::__Iterator,
        args: &[list],
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
        .unwrap()
        .unwrap()
        .unwrap()
}

// ── helpers for tests/string.rs ──

fn join_array(
    ctx: &mut StrCtx,
    delim: &'static [u8],
    parts: &[Option<&'static [u8]>],
) -> alloc::string::String {
    let d = ctx.intern(delim);
    let arr = ctx
        .arrays
        .alloc(crate::array_heap::ATYPE_REF, parts.len() as u16)
        .unwrap();
    for (i, p) in parts.iter().enumerate() {
        let raw = match p {
            Some(bytes) => {
                let Value::Reference(r) = ctx.intern(bytes) else {
                    unreachable!()
                };
                ((r as u32) | crate::array_heap::REF_TAG) as i32
            }
            None => 0,
        };
        ctx.arrays.store(arr, i, raw);
    }
    let r = ctx
        .dispatch(
            m::join,
            d::CharSequence_aCharSequence__String,
            &[d, Value::ArrayRef(arr)],
        )
        .unwrap()
        .unwrap();
    alloc::string::ToString::to_string(ctx.resolve(r))
}

// ── helpers for tests/qa_2026_09_13.rs ──

impl StrCtx {
    /// `format(...)` expecting a throw: the class name of the thrown object.
    fn fmt_throws(&mut self, fmt: &'static [u8], args: &[Value]) -> &'static str {
        let fmt_ref = self.intern(fmt);
        let arr = self.make_args(args);
        match self.dispatch(m::format, d::String_aObject__String, &[fmt_ref, arr]) {
            Err(JvmError::Exception(idx)) => self
                .objects
                .class_name(idx)
                .expect("thrown object has no class"),
            other => panic!("expected a thrown exception, got {other:?}"),
        }
    }
}

impl RngCtx {
    /// `call`, but handing back the error instead of unwrapping it.
    fn try_call(
        &mut self,
        method: &str,
        desc: &str,
        extra: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        let mut args: alloc::vec::Vec<Value> = alloc::vec![Value::ObjectRef(self.this_idx)];
        args.extend_from_slice(extra);
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: desc,
            args: &args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_Random, method, &mut ctx)
            .expect("Random method not handled")
    }
}
