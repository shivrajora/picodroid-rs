use super::*;

// goto_w forward: iconst_3 ; goto_w +6 ; iconst_2 ; ireturn
// Jumping past the iconst_2 leaves 3 on the stack.
#[test]
fn goto_w_forward_skip() {
    let code = &[
        0x06, // iconst_3            (pc=0)
        0xC8, // goto_w              (pc=1, inst_pc=1)
        0x00, 0x00, 0x00, 0x06, // offset = +6 → new pc = 7
        0x05, // iconst_2            (pc=6, skipped)
        0xAC, // ireturn             (pc=7)
    ];
    assert_eq!(run_code(1, 1, code).unwrap(), Some(Value::Int(3)));
}

// goto_w backward: jump back to an earlier instruction. Counts local_1 up to 3.
//   pc=0:  iconst_0        (1)
//   pc=1:  istore_1        (1)
//   pc=2:  iload_1         (1)
//   pc=3:  iconst_3        (1)
//   pc=4:  if_icmpge +11   (3)   if local_1 >= 3 jump to iload_1 at pc=15
//   pc=7:  iinc 1 1        (3)
//   pc=10: goto_w -8       (5)   back to pc=2
//   pc=15: iload_1         (1)
//   pc=16: ireturn         (1)
#[test]
fn goto_w_backward_loop() {
    let code = &[
        0x03, // iconst_0
        0x3C, // istore_1
        0x1B, // iload_1
        0x06, // iconst_3
        0xA2, 0x00, 0x0B, // if_icmpge +11 → 4+11 = 15 (iload_1)
        0x84, 0x01, 0x01, // iinc 1, 1
        0xC8, 0xFF, 0xFF, 0xFF, 0xF8, // goto_w -8 → 10-8 = 2
        0x1B, // iload_1
        0xAC, // ireturn
    ];
    assert_eq!(run_code(2, 2, code).unwrap(), Some(Value::Int(3)));
}

// wide iload / wide iinc: use a local index that still fits in u8 but goes
// through the wide dispatch path.
//   bipush 42, wide istore idx=5, wide iinc idx=5 +100, wide iload idx=5, ireturn
//   → 142
#[test]
fn wide_istore_iinc_iload() {
    let code = &[
        0x10, 0x2A, // bipush 42
        0xC4, 0x36, 0x00, 0x05, // wide istore 5
        0xC4, 0x84, 0x00, 0x05, 0x00, 0x64, // wide iinc 5 +100
        0xC4, 0x15, 0x00, 0x05, // wide iload 5
        0xAC, // ireturn
    ];
    assert_eq!(run_code(1, 6, code).unwrap(), Some(Value::Int(142)));
}

// wide iinc with a signed-negative constant
#[test]
fn wide_iinc_negative() {
    let code = &[
        0x10, 0x64, // bipush 100
        0x3C, // istore_1
        0xC4, 0x84, 0x00, 0x01, 0xFF, 0xF6, // wide iinc local=1 const=-10
        0x1B, // iload_1
        0xAC, // ireturn
    ];
    assert_eq!(run_code(1, 2, code).unwrap(), Some(Value::Int(90)));
}
