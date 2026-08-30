// SPDX-License-Identifier: GPL-3.0-only
//! The synchronous native→Java upcall: `ArrayList.sort(Comparator)` runs a
//! native insertion sort that calls back into interpreted `compare` for every
//! comparison.
//!
//! `ArrayList` is classfile-less, so — unlike `Collections.sort`, which is
//! ordinary Java over `Arrays` — there is no bytecode body this could live in.
//! The sort resolves at a pre-dispatch seam in `dispatch_native`, where the
//! whole `Executor` is still in hand, and re-enters the interpreter through
//! `Executor::invoke_java`.
use super::asm::{Asm, Method};
use super::*;
use crate::array_heap::ArrayHeap;
use crate::class_objects::ClassObjectCache;
use alloc::vec;

const OBJ: &str = "java/lang/Object";
const CMP_IFACE: &str = "java/util/Comparator";
const CMP_DESC: &str = "(Ljava/lang/Object;Ljava/lang/Object;)I";
const LMF_DESC: &str = "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/CallSite;";

fn hi(i: u16) -> u8 {
    (i >> 8) as u8
}
fn lo(i: u16) -> u8 {
    i as u8
}

/// A heap plus class set kept alive across `execute`, so a test can seed the
/// heap by hand and inspect it afterwards. `run_multi` builds and drops its
/// own heap internally, which is no use when the assertion is about heap
/// contents rather than a return value.
struct Harness {
    classes: Vec<ClassFile>,
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
    statics: StaticFieldStore,
    gc_state: GcState,
    class_objects: ClassObjectCache,
}

impl Harness {
    fn new(classes_data: &[&'static [u8]]) -> Self {
        let mut classes: Vec<ClassFile> = Vec::new();
        for &data in classes_data {
            classes.push(ClassFile::parse(data).expect("parse failed"));
        }
        Self {
            classes,
            strings: StringTable::new(),
            objects: ObjectHeap::new(),
            arrays: ArrayHeap::new(),
            statics: StaticFieldStore::new(),
            gc_state: GcState::new(),
            class_objects: ClassObjectCache::new(),
        }
    }

    /// Allocate a builtin `ArrayList` the way its native `<init>` does —
    /// a plain object whose field 0 holds a `list_store` buffer index.
    fn new_list(&mut self, items: &[i32]) -> (Value, u16) {
        let obj = self.objects.alloc("java/util/ArrayList").expect("alloc");
        let buf = self.objects.list_alloc().expect("list_alloc");
        self.objects.set_field(obj, 0, Value::Int(buf as i32));
        for &i in items {
            let boxed =
                helpers::box_primitive(&mut self.objects, b'I', Value::Int(i)).expect("box");
            self.objects.list_add(buf, boxed);
        }
        (Value::ObjectRef(obj), buf)
    }

    /// Read a list back as plain ints, unboxing each `Integer` by hand.
    fn list_ints(&self, buf: u16) -> Vec<i32> {
        (0..self.objects.list_len(buf))
            .map(|i| match self.objects.list_get(buf, i) {
                Some(Value::ObjectRef(idx)) => match self.objects.get_field(idx, 0) {
                    Some(Value::Int(v)) => v,
                    other => panic!("element {i} is not a boxed int: {other:?}"),
                },
                other => panic!("element {i} is not an object: {other:?}"),
            })
            .collect()
    }

    fn execute(&mut self, class_idx: usize, args: &[Value]) -> Result<Option<Value>, JvmError> {
        let mut handler = NoopHandler;
        execute(
            &self.classes,
            &mut self.strings,
            &mut self.objects,
            &mut self.arrays,
            &mut self.statics,
            &mut self.gc_state,
            &mut self.class_objects,
            &mut handler,
            class_idx,
            0,
            args,
        )
    }
}

/// `void m(List, Comparator)` — the whole body is `arg0.sort(arg1)`.
///
/// Emitted as method 0 so `Harness::execute` reaches it, on a class that also
/// carries whatever the individual test needs.
fn sort_caller(extra: &[Method<'_>], iface_of: Option<&str>) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let list = a.class("java/util/List");
    let sort = a.methodref(0x0B, list, "sort", "(Ljava/util/Comparator;)V");
    let ifaces: Vec<u16> = iface_of.map(|n| a.class(n)).into_iter().collect();
    let code = vec![
        0x2A, // aload_0 — the list
        0x2B, // aload_1 — the comparator
        0xB9,
        hi(sort),
        lo(sort),
        2,
        0,    // invokeinterface List.sort, 2 stack slots
        0xB1, // return
    ];
    let mut methods = vec![Method {
        access: 0x0009, // public static
        name: "m",
        desc: "(Ljava/util/List;Ljava/util/Comparator;)V",
        max_stack: 2,
        max_locals: 2,
        code: &code,
        exc: &[],
    }];
    methods.extend(extra.iter().map(|m| Method { ..*m }));
    a.finish_methods(0x0001, this, obj, &ifaces, &methods)
}

/// `compare(a, b)` = `a.intValue() - b.intValue()` — ascending order.
fn ascending_compare(a: &mut Asm) -> Vec<u8> {
    let integer = a.class("java/lang/Integer");
    let int_value = a.methodref(0x0A, integer, "intValue", "()I");
    vec![
        0x2B, // aload_1 — a
        0xB6,
        hi(int_value),
        lo(int_value),
        0x2C, // aload_2 — b
        0xB6,
        hi(int_value),
        lo(int_value),
        0x64, // isub
        0xAC, // ireturn
    ]
}

/// The ascending comparator as a standalone `Cmp` class.
fn ascending_comparator_class() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Cmp");
    let obj = a.class(OBJ);
    let iface = a.class(CMP_IFACE);
    let body = ascending_compare(&mut a);
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[iface],
        &[Method {
            access: 0x0001,
            name: "compare",
            desc: CMP_DESC,
            max_stack: 2,
            max_locals: 3,
            code: &body,
            exc: &[],
        }],
    )
}

// ── 1. Baseline: a bytecode class implementing Comparator ────────────────

#[test]
fn upcall_invokes_java_comparator() {
    let mut h = Harness::new(&[sort_caller(&[], None), ascending_comparator_class()]);
    let (list, buf) = h.new_list(&[5, 3, 4, 1, 2]);
    let cmp = Value::ObjectRef(h.objects.alloc("Cmp").expect("alloc"));

    h.execute(0, &[list, cmp]).expect("sort failed");
    assert_eq!(h.list_ints(buf), vec![1, 2, 3, 4, 5]);
}

#[test]
fn upcall_sorts_already_sorted_and_reversed() {
    for (input, want) in [
        (vec![1, 2, 3], vec![1, 2, 3]),
        (vec![3, 2, 1], vec![1, 2, 3]),
        (vec![7], vec![7]),
        (vec![], vec![]),
        (vec![2, 2, 1], vec![1, 2, 2]),
    ] {
        let mut h = Harness::new(&[sort_caller(&[], None), ascending_comparator_class()]);
        let (list, buf) = h.new_list(&input);
        let cmp = Value::ObjectRef(h.objects.alloc("Cmp").expect("alloc"));
        h.execute(0, &[list, cmp]).expect("sort failed");
        assert_eq!(h.list_ints(buf), want, "input {input:?}");
    }
}

// ── 2. The critical one: a lambda proxy as the comparator ────────────────

/// `Caller` with an `invokedynamic` that builds a `Comparator` lambda over a
/// static body, stores it, and sorts `arg0` with it. Exercises the path that
/// silently no-ops if `invoke_java` skips its `get_lambda` check: the nominal
/// `Comparator.compare` has no bytecode, so a name-only lookup finds an empty
/// method and the list comes back unsorted rather than erroring.
fn lambda_sort_caller(
    body_desc: &str,
    body_locals: u16,
    // Built against *this* class's constant pool: a body assembled elsewhere
    // would carry indices into a different pool.
    build_body: impl FnOnce(&mut Asm) -> Vec<u8>,
) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let lmf = a.class("java/lang/invoke/LambdaMetafactory");
    let lmf_ref = a.methodref(0x0A, lmf, "metafactory", LMF_DESC);
    let bsm = a.method_handle(6, lmf_ref);
    let body_ref = a.methodref(0x0A, this, "lam", body_desc);
    let body_handle = a.method_handle(6, body_ref);
    let sam_type = a.method_type(CMP_DESC);
    let inst_type = a.method_type(CMP_DESC);
    let indy = a.invoke_dynamic(0, "compare", "()Ljava/util/Comparator;");
    let list = a.class("java/util/List");
    let sort = a.methodref(0x0B, list, "sort", "(Ljava/util/Comparator;)V");
    let body_code = build_body(&mut a);

    let code = vec![
        0x2A, // aload_0 — the list
        0xBA,
        hi(indy),
        lo(indy),
        0,
        0, // invokedynamic → Comparator
        0xB9,
        hi(sort),
        lo(sort),
        2,
        0,
        0xB1, // return
    ];
    a.finish_full(
        0x0001,
        this,
        obj,
        &[],
        &[
            Method {
                access: 0x0009,
                name: "m",
                desc: "(Ljava/util/List;)V",
                max_stack: 3,
                max_locals: 1,
                code: &code,
                exc: &[],
            },
            Method {
                access: 0x000A, // private static
                name: "lam",
                desc: body_desc,
                max_stack: 4,
                max_locals: body_locals,
                code: &body_code,
                exc: &[],
            },
        ],
        &[(bsm, &[sam_type, body_handle, inst_type])],
    )
}

#[test]
fn upcall_invokes_lambda_comparator() {
    // Erased body: `(Object, Object)I`, unboxing both sides itself — the
    // javac shape, where the lambda body does its own conversions.
    let caller = lambda_sort_caller(CMP_DESC, 2, |a| {
        let integer = a.class("java/lang/Integer");
        let int_value = a.methodref(0x0A, integer, "intValue", "()I");
        vec![
            0x2A, // aload_0 — a (static body: no `this`)
            0xB6,
            hi(int_value),
            lo(int_value),
            0x2B, // aload_1 — b
            0xB6,
            hi(int_value),
            lo(int_value),
            0x64, // isub
            0xAC, // ireturn
        ]
    });

    let mut h = Harness::new(&[caller]);
    let (list, buf) = h.new_list(&[5, 3, 4, 1, 2]);
    h.execute(0, &[list]).expect("sort failed");
    assert_eq!(
        h.list_ints(buf),
        vec![1, 2, 3, 4, 5],
        "an unsorted list here means invoke_java resolved Comparator.compare \
         by name and found the abstract interface method instead of the \
         lambda's target body"
    );
}

// ── 3. Exceptions thrown inside an upcall ────────────────────────────────

/// A comparator whose `compare` throws. Used to prove the exception surfaces
/// at the `sort` call site rather than unwinding through the native arm.
fn throwing_comparator_class() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Cmp");
    let obj = a.class(OBJ);
    let iface = a.class(CMP_IFACE);
    let exc = a.class("java/lang/IllegalStateException");
    let ctor = a.methodref(0x0A, exc, "<init>", "()V");
    let body = vec![
        0xBB,
        hi(exc),
        lo(exc), // new IllegalStateException
        0x59,    // dup
        0xB7,
        hi(ctor),
        lo(ctor), // invokespecial <init>
        0xBF,     // athrow
    ];
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[iface],
        &[Method {
            access: 0x0001,
            name: "compare",
            desc: CMP_DESC,
            max_stack: 3,
            max_locals: 3,
            code: &body,
            exc: &[],
        }],
    )
}

#[test]
fn upcall_exception_propagates_to_outer_catch() {
    // `m(list, cmp)`: try { list.sort(cmp) } catch (Throwable) { return }
    // — the handler lives in the frame that called sort, i.e. *below* the
    // upcall. Without `handle_exception`'s floor this would either unwind
    // past the native arm or fail to find the handler at all.
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let list = a.class("java/util/List");
    let sort = a.methodref(0x0B, list, "sort", "(Ljava/util/Comparator;)V");
    let thr = a.class("java/lang/Throwable");
    let code = vec![
        0x2A, // 0: aload_0
        0x2B, // 1: aload_1
        0xB9,
        hi(sort),
        lo(sort),
        2,
        0,    // 2: invokeinterface sort
        0x04, // 7: iconst_1  (not reached — sort throws)
        0xAC, // 8: ireturn
        0x57, // 9: pop  (handler: drop the exception)
        0x05, // 10: iconst_2
        0xAC, // 11: ireturn
    ];
    let caller = a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "(Ljava/util/List;Ljava/util/Comparator;)I",
            max_stack: 3,
            max_locals: 2,
            code: &code,
            // Covers the invoke at pc 2..7, handler at pc 9.
            exc: &[[0, 7, 9, thr]],
        }],
    );

    let mut h = Harness::new(&[caller, throwing_comparator_class()]);
    let (list, _buf) = h.new_list(&[2, 1]);
    let cmp = Value::ObjectRef(h.objects.alloc("Cmp").expect("alloc"));

    let r = h.execute(0, &[list, cmp]).expect("should have been caught");
    assert_eq!(
        r,
        Some(Value::Int(2)),
        "the comparator's exception should reach the catch around sort"
    );
}

#[test]
fn upcall_uncaught_exception_propagates_out() {
    // Same throwing comparator, no handler anywhere: the error must surface
    // as an uncaught exception rather than being swallowed by the sort.
    let mut h = Harness::new(&[sort_caller(&[], None), throwing_comparator_class()]);
    let (list, _buf) = h.new_list(&[2, 1]);
    let cmp = Value::ObjectRef(h.objects.alloc("Cmp").expect("alloc"));

    match h.execute(0, &[list, cmp]) {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, "java/lang/IllegalStateException"),
        other => panic!("expected an uncaught IllegalStateException, got {other:?}"),
    }
}

// ── 4. Recursion depth ───────────────────────────────────────────────────

#[test]
fn upcall_depth_capped() {
    // A comparator whose `compare` sorts its own first argument with itself,
    // so each comparison opens a fresh upcall. Fed a self-referential list
    // (one whose elements are the list itself) that recurses without bound.
    // Without the cap it runs until the Rust stack is gone — which on target
    // is silent corruption, not a fault.
    let mut a = Asm::new();
    let this = a.class("Cmp");
    let obj = a.class(OBJ);
    let iface = a.class(CMP_IFACE);
    let list_c = a.class("java/util/List");
    let sort = a.methodref(0x0B, list_c, "sort", "(Ljava/util/Comparator;)V");
    let body = vec![
        0x2B, // aload_1 — the first element, itself a list
        0x2A, // aload_0 — this, as the comparator
        0xB9,
        hi(sort),
        lo(sort),
        2,
        0,    // recurse
        0x03, // iconst_0
        0xAC, // ireturn
    ];
    let cmp_class = a.finish_methods(
        0x0001,
        this,
        obj,
        &[iface],
        &[Method {
            access: 0x0001,
            name: "compare",
            desc: CMP_DESC,
            max_stack: 3,
            max_locals: 3,
            code: &body,
            exc: &[],
        }],
    );

    let mut h = Harness::new(&[sort_caller(&[], None), cmp_class]);
    // A list containing itself twice: sorting it compares the list with the
    // list, and the comparator sorts that — forever.
    let (list, buf) = h.new_list(&[]);
    h.objects.list_add(buf, list);
    h.objects.list_add(buf, list);
    let cmp_obj = h.objects.alloc("Cmp").expect("alloc");

    match h.execute(0, &[list, Value::ObjectRef(cmp_obj)]) {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, "java/lang/StackOverflowError"),
        other => panic!("expected StackOverflowError from the depth cap, got {other:?}"),
    }
}

// ── 5. Native dispatch keeps working during an upcall ────────────────────

/// Handler that serves one method, so a test can prove the embedder's
/// handler is still reachable from inside an upcall. A design that took the
/// handler out of the executor for the upcall's duration (`Option::take`)
/// would fail this — and would silently stop visiting the embedder's GC
/// roots at the same time.
struct CountingHandler {
    calls: u32,
}

impl NativeMethodHandler for CountingHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        if class_name == "Probe" && method_name == "tick" {
            self.calls += 1;
            return Some(Ok(Some(Value::Int(0))));
        }
        None
    }
}

#[test]
fn native_dispatch_works_during_upcall() {
    // `compare` calls a handler-served static AND a builtin (`Integer.intValue`,
    // via the ascending body), so both dispatch routes are exercised from
    // inside the nested run.
    let mut a = Asm::new();
    let this = a.class("Cmp");
    let obj = a.class(OBJ);
    let iface = a.class(CMP_IFACE);
    let probe = a.class("Probe");
    let tick = a.methodref(0x0A, probe, "tick", "()I");
    let integer = a.class("java/lang/Integer");
    let int_value = a.methodref(0x0A, integer, "intValue", "()I");
    let body = vec![
        0xB8,
        hi(tick),
        lo(tick), // invokestatic Probe.tick — handler-served
        0x57,     // pop
        0x2B,     // aload_1
        0xB6,
        hi(int_value),
        lo(int_value), // builtin
        0x2C,          // aload_2
        0xB6,
        hi(int_value),
        lo(int_value),
        0x64, // isub
        0xAC, // ireturn
    ];
    let cmp_class = a.finish_methods(
        0x0001,
        this,
        obj,
        &[iface],
        &[Method {
            access: 0x0001,
            name: "compare",
            desc: CMP_DESC,
            max_stack: 3,
            max_locals: 3,
            code: &body,
            exc: &[],
        }],
    );

    let mut h = Harness::new(&[sort_caller(&[], None), cmp_class]);
    let (list, buf) = h.new_list(&[3, 1, 2]);
    let cmp = Value::ObjectRef(h.objects.alloc("Cmp").expect("alloc"));

    let mut handler = CountingHandler { calls: 0 };
    let r = execute(
        &h.classes,
        &mut h.strings,
        &mut h.objects,
        &mut h.arrays,
        &mut h.statics,
        &mut h.gc_state,
        &mut h.class_objects,
        &mut handler,
        0,
        0,
        &[list, cmp],
    );
    r.expect("sort failed");
    assert_eq!(h.list_ints(buf), vec![1, 2, 3]);
    assert!(
        handler.calls >= 2,
        "the embedder's handler must stay reachable from inside an upcall, \
         got {} calls",
        handler.calls
    );
}

// ── 6. GC during an upcall ───────────────────────────────────────────────

/// `void m(List, Comparator)` that pushes both arguments, then **nulls both
/// locals** before calling `sort`.
///
/// This is what makes the GC test mean anything. With the arguments left in
/// locals the list is rooted by GC phase 1 (frame locals) no matter what the
/// native arm does, so the test passes whether or not shadow roots exist.
/// Cleared, and with `op_invoke` having truncated the operand stack before
/// dispatching, the list and comparator are reachable from nothing but the
/// arm's Rust locals — exactly the window `shadow_roots` covers.
fn sort_caller_clearing_locals() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let list = a.class("java/util/List");
    let sort = a.methodref(0x0B, list, "sort", "(Ljava/util/Comparator;)V");
    let code = vec![
        0x2A, // aload_0 — list onto the stack
        0x2B, // aload_1 — comparator onto the stack
        0x01,
        0x4B, // aconst_null; astore_0
        0x01,
        0x4C, // aconst_null; astore_1
        0xB9,
        hi(sort),
        lo(sort),
        2,
        0,
        0xB1, // return
    ];
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "(Ljava/util/List;Ljava/util/Comparator;)V",
            max_stack: 2,
            max_locals: 2,
            code: &code,
            exc: &[],
        }],
    )
}

/// Counts collections, so the GC test can prove one actually fired *inside*
/// the sort rather than passing because none ever ran.
struct GcCountingHandler {
    collections: u32,
}

impl NativeMethodHandler for GcCountingHandler {
    fn dispatch(
        &mut self,
        _class_name: &str,
        _method_name: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        None
    }

    fn report_gc(&mut self, _time_ns: u64, _freed: usize, _pre_gc_used: usize) {
        self.collections += 1;
    }
}

#[test]
fn upcall_survives_gc() {
    // The list is passed in as an argument and then popped off the operand
    // stack by `op_invoke` before the sort arm runs, so for the duration of
    // the sort it lives *only* in the arm's Rust locals — no frame, no field,
    // no static. `shadow_roots` is the only thing keeping it alive; without
    // it the sweep frees the backing buffer mid-sort.
    //
    // The comparator allocates hard enough to cross GC_THRESHOLD several
    // times over the course of the sort, so collections land *between*
    // comparisons. Deliberately not pre-arming `alloc_count`: that fires a
    // collection on the first opcode of the caller, before the sort begins,
    // which would leave the shadow roots untested.
    let mut a = Asm::new();
    let this = a.class("Cmp");
    let obj = a.class(OBJ);
    let iface = a.class(CMP_IFACE);
    let integer = a.class("java/lang/Integer");
    let int_value = a.methodref(0x0A, integer, "intValue", "()I");
    let junk = a.class(OBJ);
    let junk_ctor = a.methodref(0x0A, junk, "<init>", "()V");
    let mut body = vec![];
    for _ in 0..32 {
        body.extend_from_slice(&[
            0xBB,
            hi(junk),
            lo(junk), // new Object
            0x59,     // dup
            0xB7,
            hi(junk_ctor),
            lo(junk_ctor), // invokespecial <init>
            0x57,          // pop — immediately garbage
        ]);
    }
    body.extend_from_slice(&[
        0x2B,
        0xB6,
        hi(int_value),
        lo(int_value),
        0x2C,
        0xB6,
        hi(int_value),
        lo(int_value),
        0x64,
        0xAC,
    ]);
    let cmp_class = a.finish_methods(
        0x0001,
        this,
        obj,
        &[iface],
        &[Method {
            access: 0x0001,
            name: "compare",
            desc: CMP_DESC,
            max_stack: 4,
            max_locals: 3,
            code: &body,
            exc: &[],
        }],
    );

    let mut h = Harness::new(&[sort_caller_clearing_locals(), cmp_class]);
    let (list, buf) = h.new_list(&[9, 4, 7, 1, 8, 3, 6, 2, 5]);
    let cmp = Value::ObjectRef(h.objects.alloc("Cmp").expect("alloc"));

    let mut handler = GcCountingHandler { collections: 0 };
    execute(
        &h.classes,
        &mut h.strings,
        &mut h.objects,
        &mut h.arrays,
        &mut h.statics,
        &mut h.gc_state,
        &mut h.class_objects,
        &mut handler,
        0,
        0,
        &[list, cmp],
    )
    .expect("sort failed");

    assert!(
        handler.collections > 0,
        "no GC ran, so this proves nothing about shadow roots"
    );
    assert_eq!(
        h.list_ints(buf),
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        "a wrong or short result here means the list or its elements were \
         swept while the native sort held them"
    );
}

// ── 7. Argument validation ───────────────────────────────────────────────

#[test]
fn upcall_null_comparator_throws_npe() {
    let mut h = Harness::new(&[sort_caller(&[], None), ascending_comparator_class()]);
    let (list, _buf) = h.new_list(&[2, 1]);

    match h.execute(0, &[list, Value::Null]) {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, "java/lang/NullPointerException"),
        other => panic!("expected NullPointerException, got {other:?}"),
    }
}

// ── 8. The embedder-facing path: a handler arm that upcalls ──────────────

/// Serves `Probe.twice(Func, int)` by calling the Java `Func.apply(int)`
/// twice and returning the result — an *embedder-shaped* arm, reaching Java
/// through `NativeMethodHandler::invoke_java` rather than the builtin seam.
///
/// That this compiles at all is the point: `invoke_java` takes `&mut self`
/// while the arm already holds `&mut self`, so the handler reborrows itself
/// into the nested executor. A design that put the handler inside
/// `UpcallEnv` would hand out a second `&mut H` here.
struct TwiceHandler;

impl NativeMethodHandler for TwiceHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        if class_name != "Probe" || method_name != "twice" {
            return None;
        }
        let f = ctx.args[0];
        let x = ctx.args[1];
        let once = match self.invoke_java(ctx, f, "apply", "(I)I", &[x]) {
            Ok(Some(v)) => v,
            Ok(None) => return Some(Err(JvmError::InvalidReference)),
            Err(e) => return Some(Err(e)),
        };
        match self.invoke_java(ctx, f, "apply", "(I)I", &[once]) {
            Ok(Some(v)) => Some(Ok(Some(v))),
            Ok(None) => Some(Err(JvmError::InvalidReference)),
            Err(e) => Some(Err(e)),
        }
    }
}

/// `Fn` interface with `int apply(int)`, and `F implements Fn` returning
/// `x * 3`, plus a caller `m(F)I` = `Probe.twice(f, 2)`.
fn twice_classes() -> (&'static [u8], &'static [u8]) {
    let mut a = Asm::new();
    let this = a.class("F");
    let obj = a.class(OBJ);
    let iface = a.class("Fn");
    let body = vec![
        0x1B, // iload_1
        0x06, // iconst_3
        0x68, // imul
        0xAC, // ireturn
    ];
    let f = a.finish_methods(
        0x0001,
        this,
        obj,
        &[iface],
        &[Method {
            access: 0x0001,
            name: "apply",
            desc: "(I)I",
            max_stack: 2,
            max_locals: 2,
            code: &body,
            exc: &[],
        }],
    );

    let mut b = Asm::new();
    let cthis = b.class("Caller");
    let cobj = b.class(OBJ);
    let probe = b.class("Probe");
    let twice = b.methodref(0x0A, probe, "twice", "(LFn;I)I");
    let code = vec![
        0x2A, // aload_0 — the Fn
        0x05, // iconst_2
        0xB8,
        hi(twice),
        lo(twice), // invokestatic Probe.twice — no bytecode, hits the handler
        0xAC,      // ireturn
    ];
    let caller = b.finish_methods(
        0x0001,
        cthis,
        cobj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "(LFn;)I",
            max_stack: 3,
            max_locals: 1,
            code: &code,
            exc: &[],
        }],
    );
    (caller, f)
}

#[test]
fn embedder_arm_upcalls_into_java() {
    let (caller, f_class) = twice_classes();
    let mut h = Harness::new(&[caller, f_class]);
    let f = Value::ObjectRef(h.objects.alloc("F").expect("alloc"));

    let mut handler = TwiceHandler;
    let r = execute(
        &h.classes,
        &mut h.strings,
        &mut h.objects,
        &mut h.arrays,
        &mut h.statics,
        &mut h.gc_state,
        &mut h.class_objects,
        &mut handler,
        0,
        0,
        &[f],
    );
    // 2 * 3 = 6, then 6 * 3 = 18.
    assert_eq!(r.expect("upcall failed"), Some(Value::Int(18)));
}

#[test]
fn embedder_upcall_without_interpreter_fails_cleanly() {
    // A handler driven directly, with no interpreter behind it: `ctx.upcall`
    // is `None`, and `invoke_java` must say so rather than assume a frame
    // stack exists.
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let args = [Value::Null, Value::Int(1)];
    let mut ctx = NativeContext {
        descriptor: "(LFn;I)I",
        args: &args,
        strings: &mut strings,
        objects: &mut objects,
        arrays: &mut arrays,
        classes: &[],
        upcall: None,
    };
    let mut handler = TwiceHandler;
    match handler.dispatch("Probe", "twice", &mut ctx) {
        Some(Err(JvmError::NoSuchMethod)) => {}
        other => panic!("expected NoSuchMethod without an interpreter, got {other:?}"),
    }
}
