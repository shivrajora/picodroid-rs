// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::names::c;
use crate::names::spelled;

// ── Exception class "Exc" extends java/lang/Object ────────────────────────
// CP (#1..#7, cp_count=8):
//   #1 Class→#2, #2 Utf8"Exc", #3 Class→#4, #4 Utf8"java/lang/Object",
//   #5 Utf8"<init>", #6 Utf8"()V", #7 Utf8"Code"
// Method[0]: <init>()V → return
//
// Code attr len = 2+2+4+1+2+2 = 13 = 0x0D
static CLASS_EXC: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // magic + version 52
    0x00, 0x08, // cp_count=8
    0x07, 0x00, 0x02, // #1 Class→2
    0x01, 0x00, 0x03, b'E', b'x', b'c', // #2 Utf8 "Exc"
    0x07, 0x00, 0x04, // #3 Class→4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x06, b'<', b'i', b'n', b'i', b't', b'>', // #5 Utf8 "<init>"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #6 Utf8 "()V"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00,
    0x01, // method: access=1, name=#5, desc=#6, attrs=1
    0x00, 0x07, 0x00, 0x00, 0x00, 0x0D, // Code attr: name=#7, len=13
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // max_stack=1, max_locals=1, code_len=1
    0xB1, // return
    0x00, 0x00, // exc_table_len=0
    0x00, 0x00, // code_attrs_count=0
    0x00, 0x00, // class_attrs_count=0
];

// ── Test class: throw Exc, NO exception table → Err(Exception(_)) ─────────
//
// Bytecode (10 bytes, code_len=10=0x0A):
//   0: BB 00 05  new #5 (Exc)
//   3: 59        dup
//   4: B7 00 07  invokespecial #7 (Exc.<init>)
//   7: BF        athrow
//   8: 03        iconst_0 (unreachable fallthrough)
//   9: AC        ireturn
//
// Code attr len = 2+2+4+10+2+0+2 = 22 = 0x16
static CLASS_TEST_UNCAUGHT: &[u8] = &[
    // header: cp_count=14, 13 CP entries, class meta, method, Code attr name+len
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, 0x07, 0x00, 0x02, 0x01, 0x00, 0x01,
    b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g',
    b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x06, 0x01, 0x00, 0x03, b'E', b'x', b'c',
    0x0A, 0x00, 0x05, 0x00, 0x08, 0x0C, 0x00, 0x09, 0x00, 0x0A, 0x01, 0x00, 0x06, b'<', b'i', b'n',
    b'i', b't', b'>', 0x01, 0x00, 0x03, b'(', b')', b'V', 0x01, 0x00, 0x01, b'm', 0x01, 0x00, 0x03,
    b'(', b')', b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x01, 0x00, 0x01, 0x00, 0x03,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x0B, 0x00, 0x0C, 0x00, 0x01, 0x00, 0x0D,
    0x00, 0x00, 0x00, 0x16, // Code attr len=22
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0A, // max_stack=2, max_locals=1, code_len=10
    0xBB, 0x00, 0x05, // new #5 (Exc)
    0x59, // dup
    0xB7, 0x00, 0x07, // invokespecial #7 (Exc.<init>)
    0xBF, // athrow
    0x03, // iconst_0 (unreachable)
    0xAC, // ireturn
    0x00, 0x00, // exc_table_len=0
    0x00, 0x00, // code_attrs_count=0
    0x00, 0x00, // class_attrs_count=0
];

// ── Test class: throw Exc, catch Exc → Int(99) ────────────────────────────
//
// Bytecode (14 bytes):
//   0: BB 00 05  new #5 (Exc)
//   3: 59        dup
//   4: B7 00 07  invokespecial #7 (Exc.<init>)
//   7: BF        athrow      ← end of try region (end_pc=8)
//   8: 03        iconst_0    (unreachable fallthrough)
//   9: AC        ireturn
//  10: 57        pop         (handler at offset 10, catch_type=#5 "Exc")
//  11: 10 63     bipush 99
//  13: AC        ireturn
//
// Exception table: start=0, end=8, handler=10, catch_type=#5
// Code attr len = 2+2+4+14+2+8+2 = 34 = 0x22
static CLASS_TEST_CATCH: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, 0x07, 0x00, 0x02, 0x01, 0x00, 0x01,
    b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g',
    b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x06, 0x01, 0x00, 0x03, b'E', b'x', b'c',
    0x0A, 0x00, 0x05, 0x00, 0x08, 0x0C, 0x00, 0x09, 0x00, 0x0A, 0x01, 0x00, 0x06, b'<', b'i', b'n',
    b'i', b't', b'>', 0x01, 0x00, 0x03, b'(', b')', b'V', 0x01, 0x00, 0x01, b'm', 0x01, 0x00, 0x03,
    b'(', b')', b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x01, 0x00, 0x01, 0x00, 0x03,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x0B, 0x00, 0x0C, 0x00, 0x01, 0x00, 0x0D,
    0x00, 0x00, 0x00, 0x22, // Code attr len=34
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0E, // max_stack=2, max_locals=1, code_len=14
    0xBB, 0x00, 0x05, // new #5 (Exc) — offset 0
    0x59, // dup          — offset 3
    0xB7, 0x00, 0x07, // invokespecial #7 — offset 4
    0xBF, // athrow       — offset 7 (inst_pc=7, inside [0,8))
    0x03, // iconst_0     — offset 8 (unreachable)
    0xAC, // ireturn      — offset 9
    0x57, // pop (handler)— offset 10
    0x10, 0x63, // bipush 99    — offset 11
    0xAC, // ireturn      — offset 13
    0x00, 0x01, // exc_table_len=1
    0x00, 0x00, 0x00, 0x08, 0x00, 0x0A, 0x00, 0x05, // start=0,end=8,handler=10,type=#5
    0x00, 0x00, // code_attrs_count=0
    0x00, 0x00, // class_attrs_count=0
];

// ── Test class: throw Exc, catch-all (catch_type=0) → Int(99) ────────────
//
// Identical to CLASS_TEST_CATCH but exception table catch_type_index = 0
// (catch-all / finally handler)
static CLASS_TEST_CATCH_ALL: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, 0x07, 0x00, 0x02, 0x01, 0x00, 0x01,
    b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g',
    b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x06, 0x01, 0x00, 0x03, b'E', b'x', b'c',
    0x0A, 0x00, 0x05, 0x00, 0x08, 0x0C, 0x00, 0x09, 0x00, 0x0A, 0x01, 0x00, 0x06, b'<', b'i', b'n',
    b'i', b't', b'>', 0x01, 0x00, 0x03, b'(', b')', b'V', 0x01, 0x00, 0x01, b'm', 0x01, 0x00, 0x03,
    b'(', b')', b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x01, 0x00, 0x01, 0x00, 0x03,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x0B, 0x00, 0x0C, 0x00, 0x01, 0x00, 0x0D,
    0x00, 0x00, 0x00, 0x22, // len=34 (same as CLASS_TEST_CATCH)
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0E, 0xBB, 0x00, 0x05, 0x59, 0xB7, 0x00, 0x07, 0xBF,
    0x03, 0xAC, 0x57, 0x10, 0x63, 0xAC, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0A, 0x00,
    0x00, // catch_type=0 (catch-all)
    0x00, 0x00, 0x00, 0x00,
];

// ── Test class: athrow OUTSIDE the try region → Err(Exception(_)) ─────────
//
// Bytecode (17 bytes):
//    0: 00        nop  (inside try [0,3))
//    1: 00        nop
//    2: 00        nop
//    3: BB 00 05  new #5 (Exc)   ← inst_pc=3, outside [0,3)
//    6: 59        dup
//    7: B7 00 07  invokespecial #7
//   10: BF        athrow         ← inst_pc=10, NOT in [0,3) → not caught
//   11: 03        iconst_0 (unreachable)
//   12: AC        ireturn
//   13: 57        pop (handler, unreachable since exception propagates)
//   14: 10 63     bipush 99
//   16: AC        ireturn
//
// Exception table: start=0, end=3, handler=13, catch_type=#5
// Code attr len = 2+2+4+17+2+8+2 = 37 = 0x25
static CLASS_TEST_OUTSIDE_REGION: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, 0x07, 0x00, 0x02, 0x01, 0x00, 0x01,
    b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g',
    b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x06, 0x01, 0x00, 0x03, b'E', b'x', b'c',
    0x0A, 0x00, 0x05, 0x00, 0x08, 0x0C, 0x00, 0x09, 0x00, 0x0A, 0x01, 0x00, 0x06, b'<', b'i', b'n',
    b'i', b't', b'>', 0x01, 0x00, 0x03, b'(', b')', b'V', 0x01, 0x00, 0x01, b'm', 0x01, 0x00, 0x03,
    b'(', b')', b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x01, 0x00, 0x01, 0x00, 0x03,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x0B, 0x00, 0x0C, 0x00, 0x01, 0x00, 0x0D,
    0x00, 0x00, 0x00, 0x25, // Code attr len=37
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x11, // code_len=17
    0x00, // nop (inside try [0,3))
    0x00, // nop
    0x00, // nop
    0xBB, 0x00, 0x05, // new #5 (Exc) — offset 3 (outside try)
    0x59, // dup
    0xB7, 0x00, 0x07, // invokespecial #7 — offset 7
    0xBF, // athrow — offset 10 (inst_pc=10, NOT in [0,3))
    0x03, // iconst_0 (unreachable)
    0xAC, // ireturn
    0x57, // pop (handler at offset 13, unreachable)
    0x10, 0x63, // bipush 99
    0xAC, // ireturn
    0x00, 0x01, // exc_table_len=1
    0x00, 0x00, 0x00, 0x03, 0x00, 0x0D, 0x00, 0x05, // start=0,end=3,handler=13,type=#5
    0x00, 0x00, 0x00, 0x00,
];

// ── Hierarchy exception classes ───────────────────────────────────────────

// class "Base" extends java/lang/Object, <init>()V → return
static CLASS_BASE_EX: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00, 0x02, 0x01, 0x00, 0x04,
    b'B', b'a', b's', b'e', // #2 Utf8 "Base"
    0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/',
    b'O', b'b', b'j', b'e', b'c', b't', 0x01, 0x00, 0x06, b'<', b'i', b'n', b'i', b't', b'>', 0x01,
    0x00, 0x03, b'(', b')', b'V', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x01, 0x00, 0x01,
    0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01,
    0x00, 0x07, 0x00, 0x00, 0x00, 0x0D, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0xB1, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00,
];

// class "Child" extends "Base", <init>()V → return
//
// super_class = "Base" (not java/lang/Object) so is_instance_of("Child","Base") = true
static CLASS_CHILD_EX: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00, 0x02, 0x01, 0x00, 0x05,
    b'C', b'h', b'i', b'l', b'd', // #2 Utf8 "Child"
    0x07, 0x00, 0x04, 0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #4 Utf8 "Base" (super)
    0x01, 0x00, 0x06, b'<', b'i', b'n', b'i', b't', b'>', 0x01, 0x00, 0x03, b'(', b')', b'V', 0x01,
    0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x0D,
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0xB1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// ── Test class: throw Child, catch Base → Int(99) (subclass IS-A superclass)
//
// CP (#1..#15, cp_count=16=0x10):
//   #1 Class→#2 "T",  #3 Class→#4 "java/lang/Object"
//   #5 Class→#6 "Child"  ← we new and throw this
//   #7 Methodref→#5,#8  (Child.<init>:()V)
//   #8 NameAndType→#9,#10 (<init>, ()V)
//   #11 Class→#12 "Base"  ← exception table catch_type
//   #13 Utf8 "m",  #14 Utf8 "()I",  #15 Utf8 "Code"
//
// Bytecode: same 14 bytes as CLASS_TEST_CATCH but new+invokespecial on Child
// Exception table: start=0, end=8, handler=10, catch_type=#11 (Base)
// Code attr len = 34 = 0x22
static CLASS_TEST_CHILD_THROW_BASE_CATCH: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x10, // cp_count=16
    0x07, 0x00, 0x02, 0x01, 0x00, 0x01, b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v',
    b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00,
    0x06, // #5 Class→6
    0x01, 0x00, 0x05, b'C', b'h', b'i', b'l', b'd', // #6 Utf8 "Child"
    0x0A, 0x00, 0x05, 0x00, 0x08, // #7 Methodref→#5,#8
    0x0C, 0x00, 0x09, 0x00, 0x0A, // #8 NameAndType→#9,#10
    0x01, 0x00, 0x06, b'<', b'i', b'n', b'i', b't', b'>', // #9 "<init>"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #10 "()V"
    0x07, 0x00, 0x0C, // #11 Class→12
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #12 Utf8 "Base"
    0x01, 0x00, 0x01, b'm', // #13 "m"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #14 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #15 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    // Method[0]: access=1, name=#13(0x0D), desc=#14(0x0E), attrs=1
    0x00, 0x01, 0x00, 0x0D, 0x00, 0x0E, 0x00, 0x01, // Code attr: name=#15(0x0F), len=34
    0x00, 0x0F, 0x00, 0x00, 0x00, 0x22, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0E, 0xBB, 0x00,
    0x05, // new #5 (Child)
    0x59, 0xB7, 0x00, 0x07, // invokespecial #7 (Child.<init>)
    0xBF, 0x03, 0xAC, 0x57, 0x10, 0x63, 0xAC, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0A, 0x00,
    0x0B, // catch_type=#11 (Base)
    0x00, 0x00, 0x00, 0x00,
];

// ── Test class: throw Base, try to catch Child → Err(Exception(_)) ─────────
//
// Same CP structure as CLASS_TEST_CHILD_THROW_BASE_CATCH but roles reversed:
//   #5 Class→#6 "Base"   ← throw Base
//   #11 Class→#12 "Child" ← catch_type (Base is NOT-A Child → not caught)
static CLASS_TEST_BASE_THROW_CHILD_CATCH: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x10, 0x07, 0x00, 0x02, 0x01, 0x00, 0x01,
    b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g',
    b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x06, 0x01, 0x00, 0x04, b'B', b'a', b's',
    b'e', // #6 "Base" (what we throw)
    0x0A, 0x00, 0x05, 0x00, 0x08, 0x0C, 0x00, 0x09, 0x00, 0x0A, 0x01, 0x00, 0x06, b'<', b'i', b'n',
    b'i', b't', b'>', 0x01, 0x00, 0x03, b'(', b')', b'V', 0x07, 0x00, 0x0C, 0x01, 0x00, 0x05, b'C',
    b'h', b'i', b'l', b'd', // #12 "Child" (what we try to catch)
    0x01, 0x00, 0x01, b'm', 0x01, 0x00, 0x03, b'(', b')', b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd',
    b'e', 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
    0x0D, 0x00, 0x0E, 0x00, 0x01, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x22, 0x00, 0x02, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x0E, 0xBB, 0x00, 0x05, // new #5 (Base)
    0x59, 0xB7, 0x00, 0x07, // invokespecial #7 (Base.<init>)
    0xBF, 0x03, 0xAC, 0x57, 0x10, 0x63, 0xAC, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0A, 0x00,
    0x0B, // catch_type=#11 (Child)
    0x00, 0x00, 0x00, 0x00,
];

// ── Minimal class for the null-throw test ─────────────────────────────────
// Reuses the simple math-test header (cp_count=8, descriptor "()I").
// Bytecode: aconst_null (0x01), athrow (0xBF)
// Code attr len = 2+2+4+2+2+2 = 14 = 0x0E
static CLASS_ATHROW_NULL: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00, 0x02, 0x01, 0x00, 0x01,
    b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g',
    b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x01, 0x00, 0x01, b'm', 0x01, 0x00, 0x03, b'(', b')',
    b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00,
    0x00, 0x0E, // Code attr len=14
    0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, // max_stack=1, max_locals=2, code_len=2
    0x01, 0xBF, // aconst_null, athrow
    0x00, 0x00, // exc_table_len=0
    0x00, 0x00, // code_attrs_count=0
    0x00, 0x00, // class_attrs_count=0
];

// ── Tests ─────────────────────────────────────────────────────────────────

/// athrow on a null reference throws NullPointerException (JVMS; bugbash J4).
///
/// Tests the `Value::Null` branch in op_athrow.
#[test]
fn athrow_null_becomes_npe() {
    match run(CLASS_ATHROW_NULL).unwrap_err() {
        JvmError::UncaughtException {
            exception_class, ..
        } => assert_eq!(exception_class, c::java_lang_NullPointerException),
        other => panic!("expected NullPointerException, got {other:?}"),
    }
}

/// Throw an Exc with no matching exception table entry → propagates as Err.
///
/// Tests that handle_exception pops all frames and returns Err when no
/// catch entry covers the instruction PC.
#[test]
fn athrow_uncaught_propagates() {
    let result = run_multi(&[CLASS_EXC, CLASS_TEST_UNCAUGHT], 1, &[]);
    assert!(matches!(result, Err(JvmError::UncaughtException { .. })));
}

/// Throw Exc inside a try region with a matching catch handler → handler runs.
///
/// Tests that find_exception_handler finds the matching entry, clears the
/// frame stack, pushes the exception object, and sets PC to handler_pc.
#[test]
fn athrow_caught_by_matching_handler() {
    assert_eq!(
        run_multi(&[CLASS_EXC, CLASS_TEST_CATCH], 1, &[]),
        Ok(Some(Value::Int(99)))
    );
}

/// Exception table entry with catch_type_index = 0 (finally / catch-all).
///
/// Tests the `entry.catch_type_index == 0 → return Some(handler_pc)` branch.
#[test]
fn athrow_catch_all_handler() {
    assert_eq!(
        run_multi(&[CLASS_EXC, CLASS_TEST_CATCH_ALL], 1, &[]),
        Ok(Some(Value::Int(99)))
    );
}

/// athrow at inst_pc that is outside the try region (start=0, end=3) → Err.
///
/// The athrow is at offset 10, so inst_pc=10 ≥ end_pc=3 → the entry
/// does not match → exception propagates.
#[test]
fn athrow_outside_try_region() {
    let result = run_multi(&[CLASS_EXC, CLASS_TEST_OUTSIDE_REGION], 1, &[]);
    assert!(matches!(result, Err(JvmError::UncaughtException { .. })));
}

/// Throw a Child exception inside a try that catches Base.
///
/// is_instance_of(classes, "Child", "Base") must return true because
/// CLASS_CHILD_EX declares super_class = "Base" and CLASS_BASE_EX is loaded.
#[test]
fn athrow_subclass_caught_by_superclass() {
    assert_eq!(
        run_multi(
            &[
                CLASS_BASE_EX,
                CLASS_CHILD_EX,
                CLASS_TEST_CHILD_THROW_BASE_CATCH
            ],
            2,
            &[]
        ),
        Ok(Some(Value::Int(99)))
    );
}

/// Classfile-less builtin throwables resolve through the builtin hierarchy:
/// catch (Throwable) / catch (Exception) must match a RuntimeException even
/// though none of those classes have classfiles. Regression for javac's
/// synthetic try-with-resources cleanup (a generated catch (Throwable))
/// silently never firing.
#[test]
fn builtin_throwable_hierarchy_resolves_without_classfiles() {
    use crate::interpreter::helpers::is_instance_of;
    let classes: [crate::class_file::ClassFile; 0] = [];
    assert!(is_instance_of(
        &classes,
        c::java_lang_RuntimeException,
        c::java_lang_Throwable
    ));
    assert!(is_instance_of(
        &classes,
        c::java_lang_RuntimeException,
        c::java_lang_Exception
    ));
    assert!(is_instance_of(
        &classes,
        c::java_lang_NumberFormatException,
        c::java_lang_IllegalArgumentException
    ));
    assert!(is_instance_of(
        &classes,
        c::java_lang_NullPointerException,
        c::java_lang_RuntimeException
    ));
    // Object-ward only — and unrelated targets still fail.
    assert!(!is_instance_of(
        &classes,
        c::java_lang_Throwable,
        c::java_lang_Exception
    ));
    assert!(!is_instance_of(
        &classes,
        c::java_lang_RuntimeException,
        c::java_lang_Error
    ));
    // java.net taxonomy (typed network exceptions, NET-9).
    assert!(is_instance_of(
        &classes,
        c::java_net_ConnectException,
        c::java_net_SocketException
    ));
    assert!(is_instance_of(
        &classes,
        c::java_net_ConnectException,
        c::java_io_IOException
    ));
    assert!(is_instance_of(
        &classes,
        c::java_net_BindException,
        c::java_net_SocketException
    ));
    assert!(is_instance_of(
        &classes,
        c::java_net_NoRouteToHostException,
        c::java_io_IOException
    ));
    assert!(is_instance_of(
        &classes,
        c::java_net_SocketTimeoutException,
        c::java_io_InterruptedIOException
    ));
    assert!(is_instance_of(
        &classes,
        c::java_net_SocketTimeoutException,
        c::java_io_IOException
    ));
    assert!(is_instance_of(
        &classes,
        c::java_net_UnknownHostException,
        c::java_io_IOException
    ));
    assert!(is_instance_of(
        &classes,
        c::java_net_ProtocolException,
        c::java_io_IOException
    ));
    // Real-Java quirk, pinned: SocketTimeoutException extends
    // InterruptedIOException, NOT SocketException.
    assert!(!is_instance_of(
        &classes,
        c::java_net_SocketTimeoutException,
        c::java_net_SocketException
    ));
    assert!(!is_instance_of(
        &classes,
        c::java_net_SocketException,
        c::java_net_ConnectException
    ));
}

/// Throw a Base exception inside a try that only catches Child (subclass).
///
/// is_instance_of(classes, "Base", "Child") must return false — the
/// hierarchy walk goes Object-ward, not Child-ward.
#[test]
fn athrow_superclass_not_caught_by_subclass() {
    let result = run_multi(
        &[
            CLASS_BASE_EX,
            CLASS_CHILD_EX,
            CLASS_TEST_BASE_THROW_CHILD_CATCH,
        ],
        2,
        &[],
    );
    assert!(matches!(result, Err(JvmError::UncaughtException { .. })));
}

// ── Test class with LineNumberTable sub-attribute ─────────────────────────
//
// Identical to CLASS_TEST_UNCAUGHT except:
//   - cp_count: 14 → 15  (adds #14 Utf8 "LineNumberTable")
//   - Code attr len: 22 → 34  (+12 bytes for the LNT sub-attribute)
//   - code_attrs_count: 0 → 1
//   - LineNumberTable: 1 entry, start_pc=0 → line 10
//
// The athrow fires at inst_pc=7; pc_to_line(7) must return Some(10).
#[cfg(feature = "line-numbers")]
static CLASS_TEST_UNCAUGHT_WITH_LNT: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // magic + version 52
    0x00, 0x0F, // cp_count=15
    // #1..#13 identical to CLASS_TEST_UNCAUGHT
    0x07, 0x00, 0x02, 0x01, 0x00, 0x01, b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v',
    b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x06,
    0x01, 0x00, 0x03, b'E', b'x', b'c', 0x0A, 0x00, 0x05, 0x00, 0x08, 0x0C, 0x00, 0x09, 0x00, 0x0A,
    0x01, 0x00, 0x06, b'<', b'i', b'n', b'i', b't', b'>', 0x01, 0x00, 0x03, b'(', b')', b'V', 0x01,
    0x00, 0x01, b'm', 0x01, 0x00, 0x03, b'(', b')', b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e',
    // #14 Utf8 "LineNumberTable" (len=15)
    0x01, 0x00, 0x0F, b'L', b'i', b'n', b'e', b'N', b'u', b'm', b'b', b'e', b'r', b'T', b'a', b'b',
    b'l', b'e',
    // class meta: access=1, this=#1, super=#3, ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    // Method[0]: access=1, name=#11(0x0B), desc=#12(0x0C), attrs=1
    0x00, 0x01, 0x00, 0x0B, 0x00, 0x0C, 0x00, 0x01,
    // Code attr: name=#13(0x0D), len=34(0x22)
    0x00, 0x0D, 0x00, 0x00, 0x00, 0x22, // max_stack=2, max_locals=1, code_len=10
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0A, 0xBB, 0x00,
    0x05, // new #5 (Exc) — offset 0
    0x59, // dup — offset 3
    0xB7, 0x00, 0x07, // invokespecial #7 — offset 4
    0xBF, // athrow — offset 7 (inst_pc=7)
    0x03, 0xAC, // iconst_0, ireturn (unreachable)
    0x00, 0x00, // exc_table_len=0
    0x00, 0x01, // code_attrs_count=1
    0x00, 0x0E, // LNT attr_name_idx=#14
    0x00, 0x00, 0x00, 0x06, // LNT attr_len=6
    0x00, 0x01, // LNT entry_count=1
    0x00, 0x00, // start_pc=0
    0x00, 0x0A, // line_number=10
    0x00, 0x00, // class_attrs_count=0
];

// CLASS_TEST_UNCAUGHT_WITH_LNT plus a `SourceFile` attribute:
//   - cp_count: 15 → 17  (adds #15 Utf8 "SourceFile", #16 Utf8 "T.java")
//   - class_attrs_count: 0 → 1, the attribute pointing at #16
#[cfg(feature = "line-numbers")]
static CLASS_TEST_UNCAUGHT_WITH_LNT_AND_SOURCE: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // magic + version 52
    0x00, 0x11, // cp_count=17
    // #1..#14 identical to CLASS_TEST_UNCAUGHT_WITH_LNT
    0x07, 0x00, 0x02, 0x01, 0x00, 0x01, b'T', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v',
    b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x07, 0x00, 0x06,
    0x01, 0x00, 0x03, b'E', b'x', b'c', 0x0A, 0x00, 0x05, 0x00, 0x08, 0x0C, 0x00, 0x09, 0x00, 0x0A,
    0x01, 0x00, 0x06, b'<', b'i', b'n', b'i', b't', b'>', 0x01, 0x00, 0x03, b'(', b')', b'V', 0x01,
    0x00, 0x01, b'm', 0x01, 0x00, 0x03, b'(', b')', b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e',
    0x01, 0x00, 0x0F, b'L', b'i', b'n', b'e', b'N', b'u', b'm', b'b', b'e', b'r', b'T', b'a', b'b',
    b'l', b'e', // #15 Utf8 "SourceFile" (len=10)
    0x01, 0x00, 0x0A, b'S', b'o', b'u', b'r', b'c', b'e', b'F', b'i', b'l', b'e',
    // #16 Utf8 "T.java" (len=6)
    0x01, 0x00, 0x06, b'T', b'.', b'j', b'a', b'v', b'a',
    // class meta: access=1, this=#1, super=#3, ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    // Method[0]: access=1, name=#11(0x0B), desc=#12(0x0C), attrs=1
    0x00, 0x01, 0x00, 0x0B, 0x00, 0x0C, 0x00, 0x01,
    // Code attr: name=#13(0x0D), len=34(0x22)
    0x00, 0x0D, 0x00, 0x00, 0x00, 0x22, // max_stack=2, max_locals=1, code_len=10
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0A, 0xBB, 0x00,
    0x05, // new #5 (Exc) — offset 0
    0x59, // dup — offset 3
    0xB7, 0x00, 0x07, // invokespecial #7 — offset 4
    0xBF, // athrow — offset 7 (inst_pc=7)
    0x03, 0xAC, // iconst_0, ireturn (unreachable)
    0x00, 0x00, // exc_table_len=0
    0x00, 0x01, // code_attrs_count=1
    0x00, 0x0E, // LNT attr_name_idx=#14
    0x00, 0x00, 0x00, 0x06, // LNT attr_len=6
    0x00, 0x01, // LNT entry_count=1
    0x00, 0x00, // start_pc=0
    0x00, 0x0A, // line_number=10
    0x00, 0x01, // class_attrs_count=1
    0x00, 0x0F, // SourceFile attr_name_idx=#15
    0x00, 0x00, 0x00, 0x02, // attr_len=2
    0x00, 0x10, // sourcefile_index=#16
];

/// LineNumberTable parsed from Code sub-attributes → trace entry carries line number.
#[cfg(feature = "line-numbers")]
#[test]
fn uncaught_exception_trace_has_line_number() {
    let result = run_multi(&[CLASS_EXC, CLASS_TEST_UNCAUGHT_WITH_LNT], 1, &[]);
    match result {
        Err(JvmError::UncaughtException { trace, .. }) => {
            assert_eq!(trace[0].line, Some(10));
            assert_eq!(trace[0].source_file, None);
        }
        other => panic!("expected UncaughtException, got {:?}", other),
    }
}

/// A line without a SourceFile renders Android's `(Unknown Source:N)`.
#[cfg(feature = "line-numbers")]
#[test]
fn uncaught_exception_display_uses_line_format() {
    let result = run_multi(&[CLASS_EXC, CLASS_TEST_UNCAUGHT_WITH_LNT], 1, &[]);
    let s = alloc::format!("{}", result.unwrap_err());
    assert!(
        s.contains("at T.m(Unknown Source:10)"),
        "expected '(Unknown Source:10)' in '{s}'"
    );
    assert!(
        !s.contains("pc="),
        "should not contain 'pc=' when line known: '{s}'"
    );
}

/// SourceFile + LineNumberTable render the full `(File.java:N)` frame.
#[cfg(feature = "line-numbers")]
#[test]
fn uncaught_exception_display_uses_source_file() {
    let result = run_multi(
        &[CLASS_EXC, CLASS_TEST_UNCAUGHT_WITH_LNT_AND_SOURCE],
        1,
        &[],
    );
    match &result {
        Err(JvmError::UncaughtException { trace, .. }) => {
            assert_eq!(trace[0].line, Some(10));
            assert_eq!(trace[0].source_file, Some("T.java"));
        }
        other => panic!("expected UncaughtException, got {:?}", other),
    }
    let s = alloc::format!("{}", result.unwrap_err());
    assert!(
        s.contains("at T.m(T.java:10)"),
        "expected '(T.java:10)' in '{s}'"
    );
}

/// Without the feature the same class still runs and frames print `pc=`.
#[cfg(not(feature = "line-numbers"))]
#[test]
fn uncaught_exception_display_falls_back_to_pc() {
    let result = run_multi(&[CLASS_EXC, CLASS_TEST_UNCAUGHT], 1, &[]);
    let s = alloc::format!("{}", result.unwrap_err());
    assert!(s.contains("at T.m(pc=7)"), "expected '(pc=7)' in '{s}'");
}

// ── Native-minted alloc-by-name exception caught by superclass handler ────
//
// class T, method m()I:
//   try { Net.fail(); return 0; }            ← native, no classfile for Net
//   catch (java.io.IOException e) {
//     return e.getMessage() != null ? 99 : 98;
//   }
//
// The test handler intercepts Net.fail and returns a minted
// java/net/ConnectException (alloc-by-name, no classfile) exactly like the
// picodroid-core net natives do. Catching it via a catch java/io/IOException
// entry exercises builtin_super-based catch matching for a native-thrown
// exception; the getMessage() call exercises method resolution walking
// builtin_super down to Throwable's dispatcher for a classfile-less receiver.
//
// CP (#1..#19, cp_count=20=0x14):
//   #1 Class→#2 "T",  #3 Class→#4 "java/lang/Object"
//   #5 Class→#6 "Net",  #7 Methodref→#5,#8 (Net.fail:()V)
//   #8 NameAndType→#9,#10,  #9 "fail",  #10 "()V"
//   #11 Class→#12 "java/io/IOException"  ← catch_type
//   #13 Methodref→#11,#14 (getMessage:()Ljava/lang/String;)
//   #14 NameAndType→#15,#16,  #15 "getMessage",  #16 "()Ljava/lang/String;"
//   #17 "m",  #18 "()I",  #19 "Code"
//
// Bytecode (17 bytes):
//    0: B8 00 07  invokestatic #7 (Net.fail) ← throws inside try [0,3)
//    3: 03        iconst_0
//    4: AC        ireturn
//    5: B6 00 0D  invokevirtual #13 getMessage (handler entry, exc on stack)
//    8: C6 00 06  ifnull → 14
//   11: 10 63    bipush 99
//   13: AC        ireturn
//   14: 10 62    bipush 98
//   16: AC        ireturn
//
// Exception table: start=0, end=3, handler=5, catch_type=#11 (IOException)
// Code attr len = 2+2+4+17+2+8+2 = 37 = 0x25
static CLASS_TEST_NATIVE_NET_EXC: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x14, // cp_count=20
    0x07, 0x00, 0x02, // #1 Class→2
    0x01, 0x00, 0x01, b'T', // #2 "T"
    0x07, 0x00, 0x04, // #3 Class→4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 "java/lang/Object"
    0x07, 0x00, 0x06, // #5 Class→6
    0x01, 0x00, 0x03, b'N', b'e', b't', // #6 "Net"
    0x0A, 0x00, 0x05, 0x00, 0x08, // #7 Methodref→#5,#8
    0x0C, 0x00, 0x09, 0x00, 0x0A, // #8 NameAndType→#9,#10
    0x01, 0x00, 0x04, b'f', b'a', b'i', b'l', // #9 "fail"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #10 "()V"
    0x07, 0x00, 0x0C, // #11 Class→12
    0x01, 0x00, 0x13, b'j', b'a', b'v', b'a', b'/', b'i', b'o', b'/', b'I', b'O', b'E', b'x', b'c',
    b'e', b'p', b't', b'i', b'o', b'n', // #12 "java/io/IOException"
    0x0A, 0x00, 0x0B, 0x00, 0x0E, // #13 Methodref→#11,#14
    0x0C, 0x00, 0x0F, 0x00, 0x10, // #14 NameAndType→#15,#16
    0x01, 0x00, 0x0A, b'g', b'e', b't', b'M', b'e', b's', b's', b'a', b'g', b'e', // #15
    0x01, 0x00, 0x14, b'(', b')', b'L', b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/',
    b'S', b't', b'r', b'i', b'n', b'g', b';', // #16 "()Ljava/lang/String;"
    0x01, 0x00, 0x01, b'm', // #17 "m"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #18 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #19 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x11, 0x00, 0x12, 0x00, 0x01, // method: name=#17, desc=#18, attrs=1
    0x00, 0x13, 0x00, 0x00, 0x00, 0x25, // Code attr: name=#19, len=37
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x11, // max_stack=2, max_locals=1, code_len=17
    0xB8, 0x00, 0x07, // invokestatic #7 — offset 0
    0x03, // iconst_0     — offset 3
    0xAC, // ireturn      — offset 4
    0xB6, 0x00, 0x0D, // invokevirtual #13 — offset 5 (handler)
    0xC6, 0x00, 0x06, // ifnull → 14  — offset 8
    0x10, 0x63, // bipush 99    — offset 11
    0xAC, // ireturn      — offset 13
    0x10, 0x62, // bipush 98    — offset 14
    0xAC, // ireturn      — offset 16
    0x00, 0x01, // exc_table_len=1
    0x00, 0x00, 0x00, 0x03, 0x00, 0x05, 0x00, 0x0B, // start=0,end=3,handler=5,type=#11
    0x00, 0x00, // code_attrs_count=0
    0x00, 0x00, // class_attrs_count=0
];

/// Handler that mints a java/net/ConnectException the way the net natives
/// do — alloc-by-name + intern_dyn message + register_exception_message —
/// and throws it from a native method.
struct NetFailHandler;
impl NativeMethodHandler for NetFailHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        if class_name == "Net" && method_name == "fail" {
            let Some(idx) = ctx.objects.alloc(c::java_net_ConnectException) else {
                return Some(Err(JvmError::StackOverflow));
            };
            if let Some(midx) = ctx.strings.intern_dyn(b"Connection refused") {
                ctx.objects.register_exception_message(idx, midx);
            }
            return Some(Err(JvmError::Exception(idx)));
        }
        None
    }
}

/// A native-thrown java/net/ConnectException (no classfile) is caught by a
/// `catch java/io/IOException` handler (also no classfile) via builtin_super,
/// and getMessage() on the minted object resolves through the builtin
/// hierarchy to Throwable's dispatcher and returns the registered message.
#[test]
fn native_minted_net_exception_caught_by_ioexception_handler() {
    let cf = ClassFile::parse(spelled(CLASS_TEST_NATIVE_NET_EXC)).expect("parse failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut handler = NetFailHandler;
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut gc_state,
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        0,
        0,
        &[],
    );
    // 99 = caught AND getMessage() returned non-null (98 would mean a null
    // message; an Err would mean the catch never matched).
    assert_eq!(result, Ok(Some(Value::Int(99))));
}

// ── Native throw inside <init>, caught, then the same class constructed again ──
//
// The picoenvmon dashboard's bind-retry loop found this: `new ServerSocket(port)`
// whose <init> hits a throwing native (EADDRINUSE), the caller catches the
// BindException and retries `new ServerSocket` — and the second construction
// died with InvalidReference. This is the minimal shape: class C's <init> calls
// Object.<init> (native-dispatched — no classfile) then a throwing native;
// class T constructs C twice, each inside its own catch-all region, and
// returns 42.
//
// Class C — <init>()V = { super(); Net.fail(); }
// CP: #1 Class→#2 "C", #3 Class→#4 "java/lang/Object", #5 "<init>", #6 "()V",
//     #7 Methodref #3.#8 (Object.<init>), #8 NameAndType #5,#6,
//     #9 Class→#10 "Net", #11 Methodref #9.#12 (Net.fail), #12 NameAndType #13,#6,
//     #13 "fail", #14 "Code"
static CLASS_CTOR_NATIVE_THROW_C: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0F, // cp_count=15
    0x07, 0x00, 0x02, // #1 Class→#2
    0x01, 0x00, 0x01, b'C', // #2 "C"
    0x07, 0x00, 0x04, // #3 Class→#4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 "java/lang/Object"
    0x01, 0x00, 0x06, b'<', b'i', b'n', b'i', b't', b'>', // #5 "<init>"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #6 "()V"
    0x0A, 0x00, 0x03, 0x00, 0x08, // #7 Methodref #3.#8
    0x0C, 0x00, 0x05, 0x00, 0x06, // #8 NameAndType #5,#6
    0x07, 0x00, 0x0A, // #9 Class→#10
    0x01, 0x00, 0x03, b'N', b'e', b't', // #10 "Net"
    0x0A, 0x00, 0x09, 0x00, 0x0C, // #11 Methodref #9.#12
    0x0C, 0x00, 0x0D, 0x00, 0x06, // #12 NameAndType #13,#6
    0x01, 0x00, 0x04, b'f', b'a', b'i', b'l', // #13 "fail"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #14 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=public, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // <init>()V, public, 1 attr
    0x00, 0x0E, 0x00, 0x00, 0x00, 0x14, // Code attr, len=20
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, // max_stack=1, max_locals=1, code_len=8
    0x2A, // aload_0
    0xB7, 0x00, 0x07, // invokespecial Object.<init>
    0xB8, 0x00, 0x0B, // invokestatic Net.fail — throws
    0xB1, // return
    0x00, 0x00, // exc_table=0
    0x00, 0x00, // code attrs=0
    0x00, 0x00, // class attrs=0
];

// Class T — m()I = { try{ new C(); }catch(any){} try{ new C(); }catch(any){} return 42; }
// The handler pc for each region is the pop that the success path also runs,
// so both paths merge with one stack slot to drop (no verifier here).
// CP: #1 Class→#2 "T", #3 Class→#4 "java/lang/Object", #5 Class→#6 "C",
//     #7 Methodref #5.#8, #8 NameAndType #9,#10, #9 "<init>", #10 "()V",
//     #11 "m", #12 "()I", #13 "Code"
static CLASS_CTOR_NATIVE_THROW_T: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, // cp_count=14
    0x07, 0x00, 0x02, // #1 Class→#2
    0x01, 0x00, 0x01, b'T', // #2 "T"
    0x07, 0x00, 0x04, // #3 Class→#4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 "java/lang/Object"
    0x07, 0x00, 0x06, // #5 Class→#6
    0x01, 0x00, 0x01, b'C', // #6 "C"
    0x0A, 0x00, 0x05, 0x00, 0x08, // #7 Methodref #5.#8
    0x0C, 0x00, 0x09, 0x00, 0x0A, // #8 NameAndType #9,#10
    0x01, 0x00, 0x06, b'<', b'i', b'n', b'i', b't', b'>', // #9 "<init>"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #10 "()V"
    0x01, 0x00, 0x01, b'm', // #11 "m"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #12 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #13 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=public, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x0B, 0x00, 0x0C, 0x00, 0x01, // m()I, public, 1 attr
    0x00, 0x0D, 0x00, 0x00, 0x00, 0x2F, // Code attr, len=47
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x13, // max_stack=2, max_locals=1, code_len=19
    0xBB, 0x00, 0x05, // 0: new C
    0x59, // 3: dup
    0xB7, 0x00, 0x07, // 4: invokespecial C.<init>
    0x57, // 7: pop (obj on success, exc via handler)
    0xBB, 0x00, 0x05, // 8: new C
    0x59, // 11: dup
    0xB7, 0x00, 0x07, // 12: invokespecial C.<init>
    0x57, // 15: pop
    0x10, 0x2A, // 16: bipush 42
    0xAC, // 18: ireturn
    0x00, 0x02, // exc_table_len=2
    0x00, 0x00, 0x00, 0x07, 0x00, 0x07, 0x00, 0x00, // [0,7)→7 catch-all
    0x00, 0x08, 0x00, 0x0F, 0x00, 0x0F, 0x00, 0x00, // [8,15)→15 catch-all
    0x00, 0x00, // code attrs=0
    0x00, 0x00, // class attrs=0
];

/// A native throw inside `<init>` (after the native-dispatched
/// `Object.<init>`), caught by the caller, must leave the interpreter able
/// to construct the same class again — the picoenvmon ServerSocket
/// bind-retry shape.
#[test]
fn native_throw_in_ctor_then_reconstruct() {
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(ClassFile::parse(spelled(CLASS_CTOR_NATIVE_THROW_T)).expect("parse T"));
    classes.push(ClassFile::parse(spelled(CLASS_CTOR_NATIVE_THROW_C)).expect("parse C"));
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut handler = NetFailHandler;
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut gc_state,
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        0,
        0,
        &[],
    );
    assert_eq!(result, Ok(Some(Value::Int(42))));
}

/// Unbounded Java recursion must surface as a catchable `StackOverflowError`.
///
/// Before the frame-depth cap the frame stack was unbounded, so runaway
/// recursion exhausted the *heap* instead and reported a bare allocation
/// failure — after poisoning every other allocation on the way down.
#[test]
fn frame_depth_cap_throws_stack_overflow_error() {
    use super::asm::{Asm, Method};
    let mut a = Asm::new();
    let this = a.class("R");
    let obj = a.class(c::java_lang_Object);
    let me = a.methodref(0x0A, this, "m", "()I");
    let code = [
        0xB8,
        (me >> 8) as u8,
        me as u8, // invokestatic R.m()I — forever
        0xAC,     // ireturn
    ];
    let cls = a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "()I",
            max_stack: 1,
            max_locals: 1,
            code: &code,
            exc: &[],
        }],
    );

    match run_multi(&[cls], 0, &[]) {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, c::java_lang_StackOverflowError),
        other => panic!("expected StackOverflowError from the depth cap, got {other:?}"),
    }
}

// ── bugbash J4/J5: runtime faults must be catchable Java exceptions ───────

mod runtime_faults {
    use super::super::asm::Asm;
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// `body` then `iconst_1 ireturn`; when `catch` is set a handler covering
    /// the body pops the exception and returns 7 — the `cast_class` shape.
    fn faulting_class(body: &[u8], catch: Option<&str>) -> &'static [u8] {
        let mut a = Asm::new();
        let this = a.class("T");
        let obj = a.class(c::java_lang_Object);
        let c = catch.map(|c| a.class(c));
        let mut code = body.to_vec();
        code.push(0x04); // iconst_1
        code.push(0xAC); // ireturn
        let handler = code.len() as u16;
        code.push(0x57); // pop (the exception)
        code.push(0x10); // bipush
        code.push(0x07); //   7
        code.push(0xAC); // ireturn
        let exc: Vec<[u16; 4]> = c
            .map(|c| vec![[0, handler, handler, c]])
            .unwrap_or_default();
        a.finish(0x0001, this, obj, &[], Some((4, &code, &exc)))
    }

    const DIV_BY_ZERO: &[u8] = &[0x04, 0x03, 0x6C, 0x57]; // iconst_1 iconst_0 idiv pop

    #[test]
    fn division_by_zero_throws_catchable_arithmetic_exception() {
        for catch in [
            c::java_lang_ArithmeticException,
            c::java_lang_RuntimeException,
        ] {
            let r = run(faulting_class(DIV_BY_ZERO, Some(catch)));
            assert_eq!(r.unwrap(), Some(Value::Int(7)), "catch {catch}");
        }
        // lrem 1 % 0: lconst_1 lconst_0 lrem pop2
        let body = &[0x0A, 0x09, 0x71, 0x58];
        let r = run(faulting_class(body, Some(c::java_lang_ArithmeticException)));
        assert_eq!(r.unwrap(), Some(Value::Int(7)));
        match run(faulting_class(DIV_BY_ZERO, None)) {
            Err(JvmError::UncaughtException {
                exception_class, ..
            }) => assert_eq!(exception_class, c::java_lang_ArithmeticException),
            other => panic!("expected uncaught ArithmeticException, got {other:?}"),
        }
    }

    #[test]
    fn array_index_out_of_bounds_throws_catchable_exception() {
        // iconst_2; newarray int; iconst_5; iaload; pop
        let body = &[0x05, 0xBC, 0x0A, 0x08, 0x2E, 0x57];
        for catch in [
            c::java_lang_ArrayIndexOutOfBoundsException,
            c::java_lang_IndexOutOfBoundsException,
            c::java_lang_RuntimeException,
        ] {
            let r = run(faulting_class(body, Some(catch)));
            assert_eq!(r.unwrap(), Some(Value::Int(7)), "catch {catch}");
        }
        match run(faulting_class(body, None)) {
            Err(JvmError::UncaughtException {
                exception_class, ..
            }) => assert_eq!(exception_class, c::java_lang_ArrayIndexOutOfBoundsException),
            other => panic!("expected uncaught AIOOBE, got {other:?}"),
        }
    }

    #[test]
    fn athrow_null_and_null_array_throw_catchable_npe() {
        let athrow_null = &[0x01, 0xBF]; // aconst_null athrow
        let r = run(faulting_class(
            athrow_null,
            Some(c::java_lang_NullPointerException),
        ));
        assert_eq!(r.unwrap(), Some(Value::Int(7)));
        // aconst_null; iconst_0; iaload; pop — load through a null array.
        let null_load = &[0x01, 0x03, 0x2E, 0x57];
        let r = run(faulting_class(
            null_load,
            Some(c::java_lang_NullPointerException),
        ));
        assert_eq!(r.unwrap(), Some(Value::Int(7)));
        // arraylength on null.
        let null_len = &[0x01, 0xBE, 0x57];
        let r = run(faulting_class(
            null_len,
            Some(c::java_lang_NullPointerException),
        ));
        assert_eq!(r.unwrap(), Some(Value::Int(7)));
    }

    #[test]
    fn negative_array_size_throws_catchable_exception() {
        let body = &[0x02, 0xBC, 0x0A, 0x57]; // iconst_m1 newarray int pop
        let r = run(faulting_class(
            body,
            Some(c::java_lang_NegativeArraySizeException),
        ));
        assert_eq!(r.unwrap(), Some(Value::Int(7)));
    }

    #[test]
    fn oversized_array_throws_oom_instead_of_truncating() {
        // sipush 7000; bipush 10; imul → 70000; newarray byte — used to
        // silently truncate to 4464 elements.
        let body = &[0x11, 0x1B, 0x58, 0x10, 0x0A, 0x68, 0xBC, 0x08, 0x57];
        let r = run(faulting_class(body, Some(c::java_lang_OutOfMemoryError)));
        assert_eq!(r.unwrap(), Some(Value::Int(7)));
    }

    #[test]
    fn unsatisfiable_long_array_terminates_with_oom() {
        // sipush 20000; iconst_2; imul → 40000; newarray long — 80000 slots
        // > u16::MAX used to livelock the GC-retry loop forever.
        let body = &[0x11, 0x4E, 0x20, 0x05, 0x68, 0xBC, 0x0B, 0x57];
        match run(faulting_class(body, None)) {
            Err(JvmError::UncaughtException {
                exception_class, ..
            }) => assert_eq!(exception_class, c::java_lang_OutOfMemoryError),
            other => panic!("expected uncaught OutOfMemoryError, got {other:?}"),
        }
    }
}
