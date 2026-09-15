// SPDX-License-Identifier: GPL-3.0-only
use crate::types::{JvmError, MonitorKey, Value};
use alloc::vec::Vec;

pub struct Frame {
    pub class_idx: usize,
    pub method_idx: usize,
    pub pc: usize,
    /// Start PC of the most recently executed instruction.
    /// Used by the exception handler search when unwinding the frame stack.
    pub inst_pc: usize,
    pub locals: Vec<Value>,
    pub stack: Vec<Value>,
    /// Non-zero for a lambda body whose primitive return (`I`, `J`, …) must
    /// be boxed for the erased SAM (`Function1.invoke(Object)Object`) — the
    /// adaptation `LambdaMetafactory` performs on a real JVM. Applied by the
    /// return opcode.
    pub box_return: u8,
    /// The monitor an `ACC_SYNCHRONIZED` method holds for its whole
    /// activation — the receiver, or the Class object for a static method.
    /// Taken when the frame is pushed and released on every way it can
    /// leave the stack (JVMS §2.11.10); `None` for every other method.
    pub monitor: Option<MonitorKey>,
}

impl Frame {
    pub fn new(
        class_idx: usize,
        method_idx: usize,
        args: &[Value],
        max_locals: u16,
        max_stack: u16,
    ) -> Result<Self, JvmError> {
        // Category-2 slot layout: the interpreter keeps a long/double as ONE
        // Value on the operand stack (see ops_stack's is_cat2 handling), but
        // classfile LOCAL indices count them as TWO slots — a (JJ)J method
        // reads its second argument with lload_2, not lload_1. Expand each
        // cat-2 argument with a high-half filler so locals line up with
        // javac's slot numbering; verified bytecode never addresses the
        // filler slot directly. (First hit: TimeFormat.floorDiv(long, long)
        // — the second arg read back as Null.)
        // Both buffers reserve fallibly: a frame the heap cannot hold is the
        // crate's allocation-failure signal (a catchable OutOfMemoryError
        // upstairs), not an abort — a thread-heavy app on a full heap reset
        // the board through `Vec::with_capacity` here (QA 2026-09-13,
        // qa_thr on pico_touch_kit).
        let cap = (max_locals as usize).max(args.len() * 2);
        let mut locals = Vec::new();
        locals
            .try_reserve(cap)
            .map_err(|_| JvmError::StackOverflow)?;
        for v in args {
            locals.push(*v);
            if matches!(v, Value::Long(_) | Value::Double(_)) {
                locals.push(Value::Null);
            }
        }
        let cap = (max_locals as usize).max(locals.len());
        locals.resize(cap, Value::Null);
        let mut stack = Vec::new();
        stack
            .try_reserve(max_stack as usize)
            .map_err(|_| JvmError::StackOverflow)?;
        Ok(Self {
            class_idx,
            method_idx,
            pc: 0,
            inst_pc: 0,
            locals,
            stack,
            box_return: 0,
            monitor: None,
        })
    }

    #[inline]
    pub fn push(&mut self, v: Value) -> Result<(), JvmError> {
        self.stack.push(v);
        Ok(())
    }

    #[inline]
    pub fn pop(&mut self) -> Result<Value, JvmError> {
        self.stack.pop().ok_or(JvmError::StackUnderflow)
    }

    #[inline]
    pub fn load_local(&self, idx: u16) -> Result<Value, JvmError> {
        self.locals
            .get(idx as usize)
            .copied()
            .ok_or(JvmError::InvalidBytecode)
    }

    #[inline]
    pub fn store_local(&mut self, idx: u16, v: Value) -> Result<(), JvmError> {
        let i = idx as usize;
        if let Some(slot) = self.locals.get_mut(i) {
            *slot = v;
        } else {
            // Rare: idx exceeds pre-allocated max_locals (should not happen
            // with well-formed class files, but handle gracefully).
            self.locals.resize(i + 1, Value::Null);
            self.locals[i] = v;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_args_as_locals() {
        let args = [Value::Int(42), Value::Int(7)];
        let frame = Frame::new(0, 0, &args, 4, 4).expect("Frame::new should succeed");
        assert_eq!(frame.pc, 0);
        assert_eq!(frame.load_local(0), Ok(Value::Int(42)));
        assert_eq!(frame.load_local(1), Ok(Value::Int(7)));
    }

    #[test]
    fn new_expands_category2_args_to_two_slots() {
        // (JIJ)V javac layout: long a → slots 0-1, int b → slot 2,
        // long c → slots 3-4. lload_0 / iload_2 / lload_3 must all resolve.
        let args = [Value::Long(10), Value::Int(7), Value::Long(20)];
        let frame = Frame::new(0, 0, &args, 6, 4).expect("Frame::new should succeed");
        assert_eq!(frame.load_local(0), Ok(Value::Long(10)));
        assert_eq!(frame.load_local(2), Ok(Value::Int(7)));
        assert_eq!(frame.load_local(3), Ok(Value::Long(20)));
    }

    #[test]
    fn push_pop_round_trip() {
        let mut frame = Frame::new(0, 0, &[], 4, 4).expect("Frame::new should succeed");
        frame.push(Value::Int(99)).expect("push should succeed");
        assert_eq!(frame.pop(), Ok(Value::Int(99)));
    }

    #[test]
    fn pop_empty_returns_underflow() {
        let mut frame = Frame::new(0, 0, &[], 4, 4).expect("Frame::new should succeed");
        assert_eq!(frame.pop(), Err(JvmError::StackUnderflow));
    }

    #[test]
    fn push_many_succeeds() {
        let mut frame = Frame::new(0, 0, &[], 4, 4).expect("Frame::new should succeed");
        for i in 0..64 {
            assert_eq!(frame.push(Value::Int(i)), Ok(()));
        }
    }

    #[test]
    fn load_local_out_of_bounds() {
        let frame = Frame::new(0, 0, &[], 4, 4).expect("Frame::new should succeed");
        assert_eq!(frame.load_local(5), Err(JvmError::InvalidBytecode));
    }

    #[test]
    fn store_local_fills_gaps_with_null() {
        let mut frame = Frame::new(0, 0, &[], 4, 4).expect("Frame::new should succeed");
        frame
            .store_local(2, Value::Int(5))
            .expect("store_local should succeed");
        assert_eq!(frame.load_local(0), Ok(Value::Null));
        assert_eq!(frame.load_local(1), Ok(Value::Null));
        assert_eq!(frame.load_local(2), Ok(Value::Int(5)));
    }

    #[test]
    fn store_local_many_slots_succeeds() {
        let mut frame = Frame::new(0, 0, &[], 64, 4).expect("Frame::new should succeed");
        for i in 0u16..64 {
            frame
                .store_local(i, Value::Int(i as i32))
                .expect("store_local should always succeed");
        }
        assert_eq!(frame.load_local(63), Ok(Value::Int(63)));
    }
}

#[cfg(test)]
mod qa_oom_tests {
    use super::*;
    use crate::test_alloc::with_budget;

    // J-fix 935bab3e: a frame the heap cannot hold is the crate's
    // allocation-failure signal, which the main loop turns into a catchable
    // OutOfMemoryError -- it used to abort the firmware (a board reset) from
    // `Vec::with_capacity` (QA 2026-09-13, qa_thr on pico_touch_kit).
    #[test]
    fn a_frame_the_heap_cannot_hold_is_an_allocation_failure() {
        let r = with_budget(256, || Frame::new(0, 0, &[], u16::MAX, u16::MAX));
        assert!(
            matches!(r, Err(JvmError::StackOverflow)),
            "expected the allocation-failure signal, got {:?}",
            r.err()
        );
    }

    #[test]
    fn the_operand_stack_reservation_is_fallible_too() {
        // Locals fit; only `max_stack` is out of reach.
        let r = with_budget(4096, || Frame::new(0, 0, &[], 1, u16::MAX));
        assert!(
            matches!(r, Err(JvmError::StackOverflow)),
            "expected the allocation-failure signal, got {:?}",
            r.err()
        );
    }

    #[test]
    fn a_frame_that_fits_is_still_built_under_a_budget() {
        let frame = with_budget(64 * 1024, || Frame::new(0, 0, &[Value::Int(1)], 4, 4))
            .expect("a small frame must still be built");
        assert_eq!(frame.locals.len(), 4);
    }
}
