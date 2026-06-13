// SPDX-License-Identifier: GPL-3.0-only
use super::*;

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

/// athrow on a null reference → Err(InvalidReference).
///
/// Tests the `Value::Null` branch in op_athrow.
#[test]
fn athrow_null_becomes_error() {
    assert_eq!(run(CLASS_ATHROW_NULL), Err(JvmError::InvalidReference));
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
        "java/lang/RuntimeException",
        "java/lang/Throwable"
    ));
    assert!(is_instance_of(
        &classes,
        "java/lang/RuntimeException",
        "java/lang/Exception"
    ));
    assert!(is_instance_of(
        &classes,
        "java/lang/NumberFormatException",
        "java/lang/IllegalArgumentException"
    ));
    assert!(is_instance_of(
        &classes,
        "java/lang/NullPointerException",
        "java/lang/RuntimeException"
    ));
    // Object-ward only — and unrelated targets still fail.
    assert!(!is_instance_of(
        &classes,
        "java/lang/Throwable",
        "java/lang/Exception"
    ));
    assert!(!is_instance_of(
        &classes,
        "java/lang/RuntimeException",
        "java/lang/Error"
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
#[cfg(debug_assertions)]
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

/// LineNumberTable parsed from Code sub-attributes → trace entry carries line number.
#[cfg(debug_assertions)]
#[test]
fn uncaught_exception_trace_has_line_number() {
    let result = run_multi(&[CLASS_EXC, CLASS_TEST_UNCAUGHT_WITH_LNT], 1, &[]);
    match result {
        Err(JvmError::UncaughtException { trace, .. }) => {
            assert_eq!(trace[0].line, Some(10));
        }
        other => panic!("expected UncaughtException, got {:?}", other),
    }
}

/// Display of UncaughtException uses `:N` line format when LNT is present.
#[cfg(debug_assertions)]
#[test]
fn uncaught_exception_display_uses_line_format() {
    let result = run_multi(&[CLASS_EXC, CLASS_TEST_UNCAUGHT_WITH_LNT], 1, &[]);
    let s = alloc::format!("{}", result.unwrap_err());
    assert!(s.contains(":10"), "expected ':10' in '{s}'");
    assert!(
        !s.contains("pc="),
        "should not contain 'pc=' when line known: '{s}'"
    );
}
