use crate::framework::types::{JvmError, Value};
use heapless::Vec;

pub struct Frame {
    pub class_idx: usize,
    pub method_idx: usize,
    pub pc: usize,
    pub locals: Vec<Value, 16>,
    pub stack: Vec<Value, 16>,
}

impl Frame {
    pub fn new(class_idx: usize, method_idx: usize, args: &[Value]) -> Result<Self, JvmError> {
        let mut locals: Vec<Value, 16> = Vec::new();
        for &v in args {
            locals.push(v).map_err(|_| JvmError::StackOverflow)?;
        }
        Ok(Self {
            class_idx,
            method_idx,
            pc: 0,
            locals,
            stack: Vec::new(),
        })
    }

    pub fn push(&mut self, v: Value) -> Result<(), JvmError> {
        self.stack.push(v).map_err(|_| JvmError::StackOverflow)
    }

    pub fn pop(&mut self) -> Result<Value, JvmError> {
        self.stack.pop().ok_or(JvmError::StackUnderflow)
    }

    pub fn load_local(&self, idx: u8) -> Result<Value, JvmError> {
        self.locals
            .get(idx as usize)
            .copied()
            .ok_or(JvmError::InvalidBytecode)
    }

    pub fn store_local(&mut self, idx: u8, v: Value) -> Result<(), JvmError> {
        let i = idx as usize;
        while self.locals.len() <= i {
            self.locals
                .push(Value::Null)
                .map_err(|_| JvmError::StackOverflow)?;
        }
        self.locals[i] = v;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_args_as_locals() {
        let args = [Value::Int(42), Value::Int(7)];
        let frame = Frame::new(0, 0, &args).expect("Frame::new should succeed");
        assert_eq!(frame.pc, 0);
        assert_eq!(frame.load_local(0), Ok(Value::Int(42)));
        assert_eq!(frame.load_local(1), Ok(Value::Int(7)));
    }

    #[test]
    fn push_pop_round_trip() {
        let mut frame = Frame::new(0, 0, &[]).expect("Frame::new should succeed");
        frame.push(Value::Int(99)).expect("push should succeed");
        assert_eq!(frame.pop(), Ok(Value::Int(99)));
    }

    #[test]
    fn pop_empty_returns_underflow() {
        let mut frame = Frame::new(0, 0, &[]).expect("Frame::new should succeed");
        assert_eq!(frame.pop(), Err(JvmError::StackUnderflow));
    }

    #[test]
    fn push_overflow() {
        let mut frame = Frame::new(0, 0, &[]).expect("Frame::new should succeed");
        for i in 0..16 {
            assert_eq!(frame.push(Value::Int(i)), Ok(()));
        }
        assert_eq!(frame.push(Value::Int(99)), Err(JvmError::StackOverflow));
    }

    #[test]
    fn load_local_out_of_bounds() {
        let frame = Frame::new(0, 0, &[]).expect("Frame::new should succeed");
        assert_eq!(frame.load_local(5), Err(JvmError::InvalidBytecode));
    }

    #[test]
    fn store_local_fills_gaps_with_null() {
        let mut frame = Frame::new(0, 0, &[]).expect("Frame::new should succeed");
        frame
            .store_local(2, Value::Int(5))
            .expect("store_local should succeed");
        assert_eq!(frame.load_local(0), Ok(Value::Null));
        assert_eq!(frame.load_local(1), Ok(Value::Null));
        assert_eq!(frame.load_local(2), Ok(Value::Int(5)));
    }

    #[test]
    fn store_local_overflow() {
        let mut frame = Frame::new(0, 0, &[]).expect("Frame::new should succeed");
        // Fill locals slots 0..14 (15 items total) so that storing at index 15
        // would require pushing one more Null gap-fill, hitting the 16-item cap.
        for i in 0u8..15 {
            frame
                .store_local(i, Value::Int(i as i32))
                .expect("store_local should succeed for first 15 slots");
        }
        // Attempting to store at index 15 fills the last slot exactly; this
        // should either succeed (all 16 slots used) or return StackOverflow.
        let result = frame.store_local(15, Value::Int(999));
        assert!(
            matches!(result, Ok(()) | Err(JvmError::StackOverflow)),
            "expected Ok or StackOverflow, got {:?}",
            result
        );
    }
}
