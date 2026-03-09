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
