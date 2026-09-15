// SPDX-License-Identifier: GPL-3.0-only
//! What an exhausted heap does to a running program, as the 2026-09-13 QA
//! round settled it: a failed allocation inside a builtin arm collects and
//! retries, and only a collection that frees nothing becomes a catchable
//! `OutOfMemoryError`. Before the round the first refusal ended the app with
//! an uncatchable hard error, garbage or no garbage.
//!
//! [`crate::test_alloc::with_budget`] supplies the fixed heap these need.
use super::asm::{Asm, Method};
use super::*;
use crate::array_heap::ArrayHeap;
use crate::class_objects::ClassObjectCache;
use crate::names::spelled;
use crate::names::{c, d, m};
use crate::test_alloc::with_budget;
use alloc::vec;

const OBJ: &str = c::java_lang_Object;

fn hi(i: u16) -> u8 {
    (i >> 8) as u8
}
fn lo(i: u16) -> u8 {
    i as u8
}

/// `static int m(int n)`: boxes `n` distinct values, dropping each one, and
/// returns `n`; on `OutOfMemoryError` it returns -1.
///
/// Every box is garbage the moment it is popped, so a heap too small to hold
/// them all still runs the loop to the end — provided a refused allocation
/// collects and retries.
fn boxing_loop(keep: bool) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Loop");
    let obj = a.class(OBJ);
    let integer = a.class(c::java_lang_Integer);
    let oome = a.class(c::java_lang_OutOfMemoryError);
    let value_of = a.methodref(0x0A, integer, m::valueOf, d::I__Integer);
    let list = a.class(c::java_util_ArrayList);
    let list_init = a.methodref(0x0A, list, "<init>", "()V");
    let add = a.methodref(0x0A, list, m::add, d::Object__Z);

    // local 0: n (the bound), local 1: i, local 2: the retaining list.
    let mut code: Vec<u8> = vec![];
    if keep {
        code.extend_from_slice(&[
            0xBB,
            hi(list),
            lo(list),
            0x59, // new ArrayList, dup
            0xB7,
            hi(list_init),
            lo(list_init), // <init>
            0x3A,
            0x02, // astore 2
        ]);
    }
    code.extend_from_slice(&[0x03, 0x3C]); // iconst_0; istore_1
    let loop_top = code.len() as u16;
    code.extend_from_slice(&[
        0x1B, 0x1A, // iload_1, iload_0
        0xA2, 0x00, 0x00, // if_icmpge -> patched to the exit
    ]);
    let branch_at = code.len() as u16 - 3;
    if keep {
        code.extend_from_slice(&[0x19, 0x02]); // aload 2
    }
    code.extend_from_slice(&[
        0x11,
        0x03,
        0xE8, // sipush 1000 — out of the shared-box range
        0x1B,
        0x60, // iload_1, iadd
        0xB8,
        hi(value_of),
        lo(value_of), // Integer.valueOf(1000 + i)
    ]);
    if keep {
        code.extend_from_slice(&[0xB6, hi(add), lo(add)]); // list.add(box)
    }
    code.push(0x57); // pop
    code.extend_from_slice(&[0x84, 0x01, 0x01]); // iinc 1, 1
    let goto_at = code.len() as u16;
    let back = (loop_top as i32 - goto_at as i32) as i16;
    code.extend_from_slice(&[0xA7, (back >> 8) as u8, back as u8]); // goto loop_top
    let exit = code.len() as u16;
    code.extend_from_slice(&[0x1A, 0xAC]); // iload_0; ireturn
    let handler = code.len() as u16;
    code.extend_from_slice(&[0x57, 0x02, 0xAC]); // pop; iconst_m1; ireturn
    let skip = (exit as i32 - branch_at as i32) as i16;
    code[branch_at as usize + 1] = (skip >> 8) as u8;
    code[branch_at as usize + 2] = skip as u8;

    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "(I)I",
            max_stack: 4,
            max_locals: 3,
            code: &code,
            exc: &[[0, handler, handler, oome]],
        }],
    )
}

fn run_with_heap(class: &'static [u8], budget: usize, arg: i32) -> Result<Option<Value>, JvmError> {
    let cf = ClassFile::parse(spelled(class)).expect("parse failed");
    let classes: Vec<ClassFile> = vec![cf];
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut class_objects = ClassObjectCache::new();
    let mut handler = NoopHandler;
    with_budget(budget, || {
        execute(
            &classes,
            &mut strings,
            &mut objects,
            &mut arrays,
            &mut statics,
            &mut gc_state,
            &mut class_objects,
            &mut handler,
            0,
            0,
            &[Value::Int(arg)],
        )
    })
}

// J-fix 49ed4b3e: a builtin arm that cannot allocate collects and retries,
// so a loop whose boxes are all garbage runs to the end on a heap that could
// never have held them at once. Until the QA round the first refusal was a
// hard stop.
#[test]
fn a_refused_builtin_allocation_collects_and_retries() {
    // 8 KB could never hold 20,000 live boxes; it is ample for 20,000
    // successive ones, provided each refusal collects and retries.
    assert_eq!(
        run_with_heap(boxing_loop(false), 8 * 1024, 20_000),
        Ok(Some(Value::Int(20_000)))
    );
}

// J-fix 58f16fc5: when the collection frees nothing -- every box is still
// reachable -- the failure is an OutOfMemoryError the program can catch,
// not the uncatchable hard error that ended the app.
#[test]
fn an_exhausted_heap_throws_a_catchable_out_of_memory_error() {
    assert_eq!(
        run_with_heap(boxing_loop(true), 8 * 1024, 1_000_000),
        Ok(Some(Value::Int(-1))),
        "the OutOfMemoryError must reach the handler"
    );
}

/// `static int m(int n)`: `new Loop` n times, dropping each one; -1 on
/// `OutOfMemoryError`.
fn new_loop() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Loop");
    let obj = a.class(OBJ);
    let oome = a.class(c::java_lang_OutOfMemoryError);
    a.field("v", "I");

    let mut code: Vec<u8> = vec![0x03, 0x3C]; // iconst_0; istore_1
    let loop_top = code.len() as u16;
    code.extend_from_slice(&[0x1B, 0x1A, 0xA2, 0x00, 0x00]); // iload_1, iload_0, if_icmpge
    let branch_at = code.len() as u16 - 3;
    code.extend_from_slice(&[0xBB, hi(this), lo(this), 0x57]); // new Loop; pop
    code.extend_from_slice(&[0x84, 0x01, 0x01]); // iinc 1, 1
    let goto_at = code.len() as u16;
    let back = (loop_top as i32 - goto_at as i32) as i16;
    code.extend_from_slice(&[0xA7, (back >> 8) as u8, back as u8]);
    let exit = code.len() as u16;
    code.extend_from_slice(&[0x1A, 0xAC]); // iload_0; ireturn
    let handler = code.len() as u16;
    code.extend_from_slice(&[0x57, 0x02, 0xAC]); // pop; iconst_m1; ireturn
    let skip = (exit as i32 - branch_at as i32) as i16;
    code[branch_at as usize + 1] = (skip >> 8) as u8;
    code[branch_at as usize + 2] = skip as u8;

    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "(I)I",
            max_stack: 3,
            max_locals: 2,
            code: &code,
            exc: &[[0, handler, handler, oome]],
        }],
    )
}

// J-fix 58f16fc5: `new` on an exhausted heap rewinds for a collection and
// re-executes -- the `newarray` protocol -- instead of the hard stop that
// ended the app.
#[test]
fn new_on_an_exhausted_heap_collects_and_re_executes() {
    assert_eq!(
        run_with_heap(new_loop(), 8 * 1024, 20_000),
        Ok(Some(Value::Int(20_000)))
    );
}
