// SPDX-License-Identifier: GPL-3.0-only
//! `LambdaMetafactory`'s boxing adaptation for kotlinc-shaped lambdas: a
//! primitive body (`(I)I`) behind an erased SAM (`call(Object)Object`) gets
//! its arguments unboxed and its return boxed; matching descriptors and
//! captured values pass through untouched. A `this`-capturing lambda -- whose
//! body javac emits as an *instance* method, so the captured receiver is a
//! local but not a descriptor parameter -- lines its arguments up too.
use super::asm::{Asm, Method, ACC_INTERFACE};
use super::*;
use crate::names::{c, d, m};
use alloc::vec;

const OBJ: &str = c::java_lang_Object;
const LMF_DESC: &str =
    d::MethodHandles_Lookup_String_MethodType_MethodType_MethodHandle_MethodType__CallSite;

fn hi(i: u16) -> u8 {
    (i >> 8) as u8
}

fn lo(i: u16) -> u8 {
    i as u8
}

/// Interface `Func` with the single abstract `call` of descriptor `sam`.
fn func_iface(sam: &str) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Func");
    let obj = a.class(OBJ);
    a.finish_methods(
        ACC_INTERFACE,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0401,
            name: m::call,
            desc: sam,
            max_stack: 0,
            max_locals: 0,
            code: &[],
            exc: &[],
        }],
    )
}

/// Class `T` with a static lambda body `lam` (`body_desc`, `body_code`) and
/// static `m()<ret>`: `<push captures>; invokedynamic (factory_desc → Func);
/// <push arg>; invokeinterface Func.call<sam>; <tail>; <return opcode>`.
/// The bootstrap is a real `LambdaMetafactory.metafactory` handle with
/// `(sam MethodType, body handle, instantiated MethodType)` arguments.
#[allow(clippy::too_many_arguments)]
fn lambda_class(
    body_desc: &str,
    body_code: &[u8],
    body_locals: u16,
    sam: &str,
    instantiated: &str,
    factory_desc: &str,
    ret_desc: &str,
    push_captures: &[u8],
    build: impl FnOnce(&mut Asm) -> (Vec<u8>, Vec<u8>, u8),
) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let lmf = a.class(c::java_lang_invoke_LambdaMetafactory);
    let lmf_ref = a.methodref(0x0A, lmf, "metafactory", LMF_DESC);
    let bsm = a.method_handle(6, lmf_ref);
    let body_ref = a.methodref(0x0A, this, "lam", body_desc);
    let body_handle = a.method_handle(6, body_ref);
    let sam_type = a.method_type(sam);
    let inst_type = a.method_type(instantiated);
    let indy = a.invoke_dynamic(0, m::call, factory_desc);
    let func = a.class("Func");
    let call = a.methodref(0x0B, func, m::call, sam);
    let (push_arg, tail, ret_op) = build(&mut a);
    let sam_args = helpers::count_args(sam) as u8;
    let mut code = push_captures.to_vec();
    code.extend_from_slice(&[0xBA, hi(indy), lo(indy), 0, 0]);
    code.extend(push_arg);
    code.extend_from_slice(&[0xB9, hi(call), lo(call), 1 + sam_args, 0]);
    code.extend(tail);
    code.push(ret_op);
    a.finish_full(
        0x0001,
        this,
        obj,
        &[],
        &[
            // `m` first: `run_multi` executes method 0.
            Method {
                access: 0x0008,
                name: "m",
                desc: ret_desc,
                max_stack: 6,
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
                code: body_code,
                exc: &[],
            },
        ],
        &[(bsm, &[sam_type, body_handle, inst_type])],
    )
}

const SAM_OBJ: &str = d::Object__Object;

/// `bipush 21; Integer.valueOf(I)` as the argument, `checkcast Integer;
/// intValue()` as the tail, `ireturn`.
fn boxed_int_arg_int_result(a: &mut Asm) -> (Vec<u8>, Vec<u8>, u8) {
    let integer = a.class(c::java_lang_Integer);
    let value_of = a.methodref(0x0A, integer, m::valueOf, d::I__Integer);
    let int_value = a.methodref(0x0A, integer, m::intValue, "()I");
    (
        vec![0x10, 21, 0xB8, hi(value_of), lo(value_of)],
        vec![
            0xC0,
            hi(integer),
            lo(integer),
            0xB6,
            hi(int_value),
            lo(int_value),
        ],
        0xAC,
    )
}

#[test]
fn unboxes_int_argument_and_boxes_int_return() {
    // lam(I)I = arg * 2, called through call(Object)Object with Integer(21).
    let t = lambda_class(
        "(I)I",
        &[0x1A, 0x05, 0x68, 0xAC], // iload_0; iconst_2; imul; ireturn
        1,
        SAM_OBJ,
        d::Integer__Integer,
        "()LFunc;",
        "()I",
        &[],
        boxed_int_arg_int_result,
    );
    let classes = [func_iface(SAM_OBJ), t];
    assert_eq!(run_multi(&classes, 1, &[]).unwrap(), Some(Value::Int(42)));
}

#[test]
fn boxes_return_of_no_arg_body() {
    // lam()I = 5, called through call()Object.
    let sam = d::__Object;
    let t = lambda_class(
        "()I",
        &[0x08, 0xAC], // iconst_5; ireturn
        0,
        sam,
        d::__Integer,
        "()LFunc;",
        "()I",
        &[],
        |a| {
            let integer = a.class(c::java_lang_Integer);
            let int_value = a.methodref(0x0A, integer, m::intValue, "()I");
            (
                vec![],
                vec![
                    0xC0,
                    hi(integer),
                    lo(integer),
                    0xB6,
                    hi(int_value),
                    lo(int_value),
                ],
                0xAC,
            )
        },
    );
    let classes = [func_iface(sam), t];
    assert_eq!(run_multi(&classes, 1, &[]).unwrap(), Some(Value::Int(5)));
}

#[test]
fn unboxes_long_argument_and_boxes_long_return() {
    // lam(J)J = arg + 1, called with Long(3) → Long(4).
    let t = lambda_class(
        "(J)J",
        &[0x1E, 0x0A, 0x61, 0xAD], // lload_0; lconst_1; ladd; lreturn
        2,
        SAM_OBJ,
        d::Long__Long,
        "()LFunc;",
        "()J",
        &[],
        |a| {
            let long = a.class(c::java_lang_Long);
            let value_of = a.methodref(0x0A, long, m::valueOf, d::J__Long);
            let long_value = a.methodref(0x0A, long, m::longValue, "()J");
            (
                vec![0x06, 0x85, 0xB8, hi(value_of), lo(value_of)], // iconst_3; i2l; valueOf
                vec![
                    0xC0,
                    hi(long),
                    lo(long),
                    0xB6,
                    hi(long_value),
                    lo(long_value),
                ],
                0xAD,
            )
        },
    );
    let classes = [func_iface(SAM_OBJ), t];
    assert_eq!(run_multi(&classes, 1, &[]).unwrap(), Some(Value::Long(4)));
}

#[test]
fn captured_values_pass_through_untouched() {
    // lam(II)I = captured + arg; capture 40 (raw int), arg Integer(2) → 42.
    let t = lambda_class(
        "(II)I",
        &[0x1A, 0x1B, 0x60, 0xAC], // iload_0; iload_1; iadd; ireturn
        2,
        SAM_OBJ,
        d::Integer__Integer,
        "(I)LFunc;",
        "()I",
        &[0x10, 40], // bipush 40
        |a| {
            let integer = a.class(c::java_lang_Integer);
            let value_of = a.methodref(0x0A, integer, m::valueOf, d::I__Integer);
            let int_value = a.methodref(0x0A, integer, m::intValue, "()I");
            (
                vec![0x05, 0xB8, hi(value_of), lo(value_of)], // iconst_2; valueOf
                vec![
                    0xC0,
                    hi(integer),
                    lo(integer),
                    0xB6,
                    hi(int_value),
                    lo(int_value),
                ],
                0xAC,
            )
        },
    );
    let classes = [func_iface(SAM_OBJ), t];
    assert_eq!(run_multi(&classes, 1, &[]).unwrap(), Some(Value::Int(42)));
}

#[test]
fn matching_primitive_descriptors_need_no_adaptation() {
    // A `fun interface` shape: call(I)I with body (I)I.
    let sam = "(I)I";
    let t = lambda_class(
        "(I)I",
        &[0x1A, 0x05, 0x68, 0xAC],
        1,
        sam,
        sam,
        "()LFunc;",
        "()I",
        &[],
        |_| (vec![0x10, 21], vec![], 0xAC),
    );
    let classes = [func_iface(sam), t];
    assert_eq!(run_multi(&classes, 1, &[]).unwrap(), Some(Value::Int(42)));
}

#[test]
fn null_for_a_primitive_parameter_throws_npe() {
    let t = lambda_class(
        "(I)I",
        &[0x1A, 0xAC],
        1,
        SAM_OBJ,
        d::Integer__Integer,
        "()LFunc;",
        "()I",
        &[],
        |_| (vec![0x01], vec![0x57, 0x03], 0xAC), // aconst_null; …; pop; iconst_0
    );
    let classes = [func_iface(SAM_OBJ), t];
    match run_multi(&classes, 1, &[]) {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, c::java_lang_NullPointerException),
        other => panic!("expected an uncaught NPE, got {other:?}"),
    }
}

#[test]
fn instance_body_receiver_capture_consumes_no_parameter() {
    // A lambda that captures `this` (`picodroid.widget.RadioGroup` wires one
    // onto every button it tracks) compiles to a *non-static* body: javac
    // passes the receiver as local 0, where it occupies no slot of the body's
    // descriptor. Counting it as a leading parameter anyway walked one kind
    // too far and lined every remaining argument up against the wrong one --
    // here it would leave Integer(21) boxed, and RadioGroup's real
    // `(CompoundButton, boolean)` listener had its button unboxed as a `Z`.
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let lmf = a.class(c::java_lang_invoke_LambdaMetafactory);
    let lmf_ref = a.methodref(0x0A, lmf, "metafactory", LMF_DESC);
    let bsm = a.method_handle(6, lmf_ref);
    // REF_invokeSpecial (7) on a private instance body, as javac emits.
    let body_ref = a.methodref(0x0A, this, "lam", "(I)I");
    let body_handle = a.method_handle(7, body_ref);
    let sam_type = a.method_type(SAM_OBJ);
    let inst_type = a.method_type(d::Integer__Integer);
    let indy = a.invoke_dynamic(0, m::call, d::Object__Func);
    let func = a.class("Func");
    let call = a.methodref(0x0B, func, m::call, SAM_OBJ);
    let integer = a.class(c::java_lang_Integer);
    let value_of = a.methodref(0x0A, integer, m::valueOf, d::I__Integer);
    let int_value = a.methodref(0x0A, integer, m::intValue, "()I");

    // The captured receiver is only ever a local the body ignores, so any
    // object stands in for `this`: Integer(0) saves T needing an <init>.
    let mut code = vec![0x03, 0xB8, hi(value_of), lo(value_of)]; // iconst_0; valueOf
    code.extend_from_slice(&[0xBA, hi(indy), lo(indy), 0, 0]); // invokedynamic
    code.extend_from_slice(&[0x10, 21, 0xB8, hi(value_of), lo(value_of)]); // bipush 21; valueOf
    code.extend_from_slice(&[0xB9, hi(call), lo(call), 2, 0]); // invokeinterface Func.call
    code.extend_from_slice(&[0xC0, hi(integer), lo(integer)]); // checkcast Integer
    code.extend_from_slice(&[0xB6, hi(int_value), lo(int_value), 0xAC]); // intValue; ireturn
    let t = a.finish_full(
        0x0001,
        this,
        obj,
        &[],
        &[
            Method {
                access: 0x0008,
                name: "m",
                desc: "()I",
                max_stack: 6,
                max_locals: 1,
                code: &code,
                exc: &[],
            },
            Method {
                access: 0x0002, // private, *not* static
                name: "lam",
                desc: "(I)I",
                max_stack: 4,
                max_locals: 2, // 0 = the captured receiver, 1 = the int
                code: &[0x1B, 0x05, 0x68, 0xAC], // iload_1; iconst_2; imul; ireturn
                exc: &[],
            },
        ],
        &[(bsm, &[sam_type, body_handle, inst_type])],
    );
    let classes = [func_iface(SAM_OBJ), t];
    assert_eq!(run_multi(&classes, 1, &[]).unwrap(), Some(Value::Int(42)));
}
