// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// dup_x1: .., v2, v1 → .., v1, v2, v1 (cat-1). Push 1, 2, dup_x1, iadd, iadd.
// After push: [1, 2]
// dup_x1:     [2, 1, 2]
// iadd:       [2, 3]
// iadd:       [5]
#[test]
fn dup_x1() {
    let code = &[
        0x04, // iconst_1
        0x05, // iconst_2
        0x5A, // dup_x1
        0x60, // iadd
        0x60, // iadd
        0xAC, // ireturn
    ];
    assert_eq!(run_code(3, 1, code).unwrap(), Some(Value::Int(5)));
}

// dup_x2 with all cat-1: .., v3, v2, v1 → .., v1, v3, v2, v1.
// Push 1, 2, 3, 4. dup_x2 on top of 2, 3, 4.
// After push 1, 2, 3, 4: [1, 2, 3, 4]
// dup_x2: pop v1=4, v2=3, v3=2 → push 4, 2, 3, 4 → [1, 4, 2, 3, 4]
// iadd x4 reduces to sum = 1 + 4 + 2 + 3 + 4 = 14
#[test]
fn dup_x2_cat1() {
    let code = &[
        0x04, 0x05, 0x06, 0x07, // iconst_1..iconst_4
        0x5B, // dup_x2
        0x60, 0x60, 0x60, 0x60, // iadd x4
        0xAC, // ireturn
    ];
    assert_eq!(run_code(5, 1, code).unwrap(), Some(Value::Int(14)));
}

// dup2 with cat-2 top: .., v1_long → .., v1_long, v1_long.
// lconst_1, dup2, ladd, l2i → (1+1) = 2.
#[test]
fn dup2_cat2() {
    let code = &[
        0x0A, // lconst_1
        0x5C, // dup2
        0x61, // ladd
        0x88, // l2i
        0xAC, // ireturn
    ];
    assert_eq!(run_code(4, 1, code).unwrap(), Some(Value::Int(2)));
}

// swap: .., v2, v1 → .., v1, v2. Verify with subtraction where order matters.
// [5, 2] → swap → [2, 5] → isub → [2 - 5] = -3.
#[test]
fn swap_reverses_order() {
    let code = &[
        0x08, // iconst_5
        0x05, // iconst_2
        0x5F, // swap
        0x64, // isub
        0xAC, // ireturn
    ];
    assert_eq!(run_code(2, 1, code).unwrap(), Some(Value::Int(-3)));
}

// pop2 with cat-2 top should pop just one slot (long) and leave v underneath.
// iconst_7, lconst_1, pop2, ireturn → returns 7.
#[test]
fn pop2_cat2_pops_one_slot() {
    let code = &[
        0x10, 0x07, // bipush 7
        0x0A, // lconst_1
        0x58, // pop2
        0xAC, // ireturn
    ];
    assert_eq!(run_code(3, 1, code).unwrap(), Some(Value::Int(7)));
}
