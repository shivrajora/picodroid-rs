use crate::framework::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub trait NativeMethodHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
        strings: &mut StringTable,
        objects: &mut ObjectHeap,
    ) -> Result<Option<Value>, JvmError>;
}
