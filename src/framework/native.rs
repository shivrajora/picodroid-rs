use crate::framework::types::{JvmError, Value};

pub fn dispatch_native(
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    args: &[Value],
    strings: &crate::framework::heap::StringTable,
) -> Result<Option<Value>, JvmError> {
    match (class_name, method_name, descriptor) {
        ("picodroid/util/Log", "i", "(Ljava/lang/String;Ljava/lang/String;)V") => {
            crate::system::picodroid::util::log::log_i(args, strings).map(|_| None)
        }
        _ => Err(JvmError::NoSuchMethod),
    }
}
