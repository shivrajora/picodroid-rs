use crate::framework::{
    array_heap::ArrayHeap,
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
        arrays: &mut ArrayHeap,
    ) -> Result<Option<Value>, JvmError>;
}
