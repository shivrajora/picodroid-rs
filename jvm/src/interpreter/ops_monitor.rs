use super::Executor;
use crate::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, MonitorKey, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    /// monitorenter (0xc2): pop objectref and acquire its monitor.
    pub(super) fn op_monitorenter(&mut self, frame: &mut Frame) -> Result<(), JvmError> {
        let val = frame.pop()?;
        let key = value_to_monitor_key(val)?;
        self.handler.monitor_enter(key)
    }

    /// monitorexit (0xc3): pop objectref and release its monitor.
    pub(super) fn op_monitorexit(&mut self, frame: &mut Frame) -> Result<(), JvmError> {
        let val = frame.pop()?;
        let key = value_to_monitor_key(val)?;
        self.handler.monitor_exit(key)
    }
}

fn value_to_monitor_key(val: Value) -> Result<MonitorKey, JvmError> {
    match val {
        Value::ObjectRef(idx) => Ok(MonitorKey::Object(idx)),
        Value::ArrayRef(idx) => Ok(MonitorKey::Array(idx)),
        Value::Reference(idx) => Ok(MonitorKey::String(idx)),
        Value::Null => Err(JvmError::InvalidReference),
        _ => Err(JvmError::InvalidBytecode),
    }
}
