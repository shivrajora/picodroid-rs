use crate::types::{JvmError, Value};
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
}

impl Frame {
    pub fn new(
        class_idx: usize,
        method_idx: usize,
        args: &[Value],
        max_locals: u8,
        max_stack: u8,
    ) -> Result<Self, JvmError> {
        let cap = (max_locals as usize).max(args.len());
        let mut locals = Vec::with_capacity(cap);
        locals.extend_from_slice(args);
        locals.resize(cap, Value::Null);
        Ok(Self {
            class_idx,
            method_idx,
            pc: 0,
            inst_pc: 0,
            locals,
            stack: Vec::with_capacity(max_stack as usize),
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
    pub fn load_local(&self, idx: u8) -> Result<Value, JvmError> {
        self.locals
            .get(idx as usize)
            .copied()
            .ok_or(JvmError::InvalidBytecode)
    }

    #[inline]
    pub fn store_local(&mut self, idx: u8, v: Value) -> Result<(), JvmError> {
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
        for i in 0u8..64 {
            frame
                .store_local(i, Value::Int(i as i32))
                .expect("store_local should always succeed");
        }
        assert_eq!(frame.load_local(63), Ok(Value::Int(63)));
    }
}
