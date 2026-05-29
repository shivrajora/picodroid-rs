// SPDX-License-Identifier: GPL-3.0-only
//! Tests for interpreter-level invariants across multiple `execute()` calls.
//!
//! Per-call state (frames, caches) is freshly built inside `execute()`, but
//! `StaticFieldStore`, `GcState`, `StringTable`, `ObjectHeap`, and the like
//! are owned by the caller and persist. Several recent bug categories
//! (clinit re-entry, GC threshold reset, allocation counter drift) live in
//! the seam between these two lifetimes — these tests pin down the contract.

use super::*;
use crate::array_heap::ArrayHeap;
use crate::class_objects::ClassObjectCache;
use crate::gc::GcState;

/// Single-class fixture: class "Lit" with static method m()I returning iconst_5.
/// Smallest possible bytecode for repeated-invocation testing.
///
/// CP (cp_count=8):
///   #1: Class -> #2 (Lit)
///   #2: Utf8  "Lit"
///   #3: Class -> #4 (java/lang/Object)
///   #4: Utf8  "java/lang/Object"
///   #5: Utf8  "m"
///   #6: Utf8  "()I"
///   #7: Utf8  "Code"
static CLASS_LIT: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, // cp_count=8
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x03, b'L', b'i', b't', // #2 Utf8 "Lit"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #6 Utf8 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    // method: access=0x0009 (public|static), name=#5, desc=#6, attrs=1
    0x00, 0x09, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, // Code attr name=#7
    0x00, 0x00, 0x00, 0x0E, // attr_len=14
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, // max_stack=1, max_locals=0, code_len=2
    0x08, 0xAC, // iconst_5, ireturn
    0x00, 0x00, 0x00, 0x00, // exc=0, code_attrs=0
    0x00, 0x00, // class_attrs=0
];

fn fresh_state() -> (
    StringTable,
    ObjectHeap,
    ArrayHeap,
    StaticFieldStore,
    GcState,
    ClassObjectCache,
) {
    (
        StringTable::new(),
        ObjectHeap::new(),
        ArrayHeap::new(),
        StaticFieldStore::new(),
        GcState::new(),
        ClassObjectCache::new(),
    )
}

#[test]
fn execute_returns_none_for_void_with_no_return_value() {
    // Lit.m()I returns an int; reuse for a sanity check that single-class
    // execute() works as a baseline before the more complex multi-call tests.
    let cf = ClassFile::parse(CLASS_LIT).expect("parse");
    let classes = alloc::vec![cf];
    let (mut s, mut o, mut a, mut st, mut gc, mut co) = fresh_state();
    let mut h = NoopHandler;
    let r = execute(
        &classes,
        &mut s,
        &mut o,
        &mut a,
        &mut st,
        &mut gc,
        &mut co,
        &mut h,
        0,
        0,
        &[],
    );
    assert_eq!(r.unwrap(), Some(Value::Int(5)));
}

#[test]
fn repeated_invocation_returns_same_result() {
    // Calling the same method twice with the same state must yield the same
    // value. Guards against caches / counters that would diverge on a hit.
    let cf = ClassFile::parse(CLASS_LIT).expect("parse");
    let classes = alloc::vec![cf];
    let (mut s, mut o, mut a, mut st, mut gc, mut co) = fresh_state();
    let mut h = NoopHandler;
    for _ in 0..3 {
        let r = execute(
            &classes,
            &mut s,
            &mut o,
            &mut a,
            &mut st,
            &mut gc,
            &mut co,
            &mut h,
            0,
            0,
            &[],
        );
        assert_eq!(r.unwrap(), Some(Value::Int(5)));
    }
}

#[test]
fn statics_persist_across_execute_calls() {
    // Reuses CLASS_E + CLASS_CALLER from clinit.rs. First call triggers
    // E.<clinit> which sets E.val=99. The second call must reuse the
    // already-initialized static — not panic, not reset, not re-run clinit.
    let cf_e = ClassFile::parse(super::clinit::CLASS_E).expect("parse E");
    let cf_caller = ClassFile::parse(super::clinit::CLASS_CALLER).expect("parse Caller");
    let classes = alloc::vec![cf_e, cf_caller];
    let (mut s, mut o, mut a, mut st, mut gc, mut co) = fresh_state();
    let mut h = NoopHandler;

    assert!(!st.is_initialized(b"E"));
    let r1 = execute(
        &classes,
        &mut s,
        &mut o,
        &mut a,
        &mut st,
        &mut gc,
        &mut co,
        &mut h,
        1,
        0,
        &[],
    );
    assert_eq!(r1.unwrap(), Some(Value::Int(99)));
    assert!(st.is_initialized(b"E"));

    let entries_after_first = st.values_iter().count();
    let r2 = execute(
        &classes,
        &mut s,
        &mut o,
        &mut a,
        &mut st,
        &mut gc,
        &mut co,
        &mut h,
        1,
        0,
        &[],
    );
    assert_eq!(r2.unwrap(), Some(Value::Int(99)));
    assert_eq!(
        st.values_iter().count(),
        entries_after_first,
        "clinit must not re-run and create duplicate static entries on the second call"
    );
}

#[test]
fn gc_state_alloc_count_persists_across_execute_calls() {
    // The interpreter increments gc_state.alloc_count for every heap
    // allocation. Persisting that counter across calls is critical: a stream
    // of short native-driven invocations would never trip the 256-alloc GC
    // threshold otherwise. Pre-seed the counter, run a trivial method that
    // does no allocations, and verify the seed survives.
    let cf = ClassFile::parse(CLASS_LIT).expect("parse");
    let classes = alloc::vec![cf];
    let (mut s, mut o, mut a, mut st, mut gc, mut co) = fresh_state();
    let mut h = NoopHandler;
    gc.alloc_count = 200;
    let r = execute(
        &classes,
        &mut s,
        &mut o,
        &mut a,
        &mut st,
        &mut gc,
        &mut co,
        &mut h,
        0,
        0,
        &[],
    );
    assert_eq!(r.unwrap(), Some(Value::Int(5)));
    assert_eq!(
        gc.alloc_count, 200,
        "alloc_count must not reset across execute() — a no-alloc method should leave it untouched"
    );
}

#[test]
fn need_gc_flag_triggers_collection_and_clears() {
    // The need_gc flag is the "allocator hit OOM, please GC now" signal.
    // execute() must observe it on entry, run a GC cycle, then clear both
    // the flag and the alloc_count so the next allocator call has a chance
    // to succeed. Verify both fields are reset after the call.
    let cf = ClassFile::parse(CLASS_LIT).expect("parse");
    let classes = alloc::vec![cf];
    let (mut s, mut o, mut a, mut st, mut gc, mut co) = fresh_state();
    let mut h = NoopHandler;
    gc.need_gc = true;
    gc.alloc_count = 50;
    let r = execute(
        &classes,
        &mut s,
        &mut o,
        &mut a,
        &mut st,
        &mut gc,
        &mut co,
        &mut h,
        0,
        0,
        &[],
    );
    assert!(r.is_ok());
    assert!(
        !gc.need_gc,
        "need_gc must be cleared after execute() runs the requested emergency GC"
    );
    assert_eq!(
        gc.alloc_count, 0,
        "alloc_count must be reset to 0 after a GC cycle so the next threshold check starts fresh"
    );
}

#[test]
fn object_heap_persists_across_execute_calls() {
    // Pre-allocating an object in the heap before execute() must not be
    // disturbed by the call. This is the contract sensors / lifecycle code
    // relies on when delivering events into a Java callback.
    let cf = ClassFile::parse(CLASS_LIT).expect("parse");
    let classes = alloc::vec![cf];
    let (mut s, mut o, mut a, mut st, mut gc, mut co) = fresh_state();
    let mut h = NoopHandler;
    let pre = o.alloc("PreExisting").unwrap();
    assert!(o.is_live(pre));
    let r = execute(
        &classes,
        &mut s,
        &mut o,
        &mut a,
        &mut st,
        &mut gc,
        &mut co,
        &mut h,
        0,
        0,
        &[],
    );
    assert!(r.is_ok());
    assert!(
        o.is_live(pre),
        "pre-existing object must survive an execute() call"
    );
    assert_eq!(o.class_name(pre), Some("PreExisting"));
}

#[test]
fn invokevirtual_dispatches_consistently_across_repeated_calls() {
    // First call seeds whatever method-resolution state lives behind the
    // call. The second call must dispatch the same override and return the
    // same value — pins down the invariant that overrides don't drift on a
    // warm cache.
    let cf_base = ClassFile::parse(super::invoke::CLASS_BASE_SPEAK).expect("parse Base");
    let cf_child = ClassFile::parse(super::invoke::CLASS_CHILD_SPEAK).expect("parse Child");
    let cf_caller =
        ClassFile::parse(super::invoke::CLASS_CALLER_INVOKEVIRTUAL).expect("parse Caller");
    let classes = alloc::vec![cf_base, cf_child, cf_caller];
    let (mut s, mut o, mut a, mut st, mut gc, mut co) = fresh_state();
    let mut h = NoopHandler;

    for _ in 0..3 {
        let obj = alloc_object(&mut o, "Child");
        let r = execute(
            &classes,
            &mut s,
            &mut o,
            &mut a,
            &mut st,
            &mut gc,
            &mut co,
            &mut h,
            2,
            0,
            &[obj],
        );
        assert_eq!(r.unwrap(), Some(Value::Int(2)));
    }
}

#[test]
fn invokevirtual_switches_dispatch_when_receiver_class_changes() {
    // Same call site invoked first with a Child receiver (→2), then with a
    // ChildNS receiver (→1, walks to Base). Verifies that virtual dispatch
    // re-resolves per-receiver and does not lock onto the first target.
    use crate::interpreter::tests::invoke::{
        CLASS_BASE_SPEAK, CLASS_CALLER_INVOKEVIRTUAL, CLASS_CHILD_SPEAK,
    };
    let cf_base = ClassFile::parse(CLASS_BASE_SPEAK).expect("parse Base");
    let cf_child = ClassFile::parse(CLASS_CHILD_SPEAK).expect("parse Child");
    let cf_caller = ClassFile::parse(CLASS_CALLER_INVOKEVIRTUAL).expect("parse Caller");
    let classes = alloc::vec![cf_base, cf_child, cf_caller];
    let (mut s, mut o, mut a, mut st, mut gc, mut co) = fresh_state();
    let mut h = NoopHandler;

    let child = alloc_object(&mut o, "Child");
    let r_child = execute(
        &classes,
        &mut s,
        &mut o,
        &mut a,
        &mut st,
        &mut gc,
        &mut co,
        &mut h,
        2,
        0,
        &[child],
    );
    assert_eq!(r_child.unwrap(), Some(Value::Int(2)));

    let base = alloc_object(&mut o, "Base");
    let r_base = execute(
        &classes,
        &mut s,
        &mut o,
        &mut a,
        &mut st,
        &mut gc,
        &mut co,
        &mut h,
        2,
        0,
        &[base],
    );
    assert_eq!(r_base.unwrap(), Some(Value::Int(1)));
}
