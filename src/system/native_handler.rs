use crate::framework::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
    NativeMethodHandler,
};

pub struct PicodroidNativeHandler;

impl NativeMethodHandler for PicodroidNativeHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        _descriptor: &str,
        args: &[Value],
        strings: &StringTable,
        objects: &mut ObjectHeap,
    ) -> Result<Option<Value>, JvmError> {
        match (class_name, method_name) {
            ("picodroid/util/Log", "i") => {
                crate::system::picodroid::util::log::log_i(args, strings).map(|_| None)
            }
            ("picodroid/pio/PeripheralManager", "getInstance") => {
                crate::system::picodroid::pio::peripheral_manager::get_instance(objects)
            }
            ("picodroid/pio/PeripheralManager", "openGpio") => {
                crate::system::picodroid::pio::peripheral_manager::open_gpio(args, strings, objects)
            }
            ("picodroid/pio/Gpio", "setDirection") => {
                crate::system::picodroid::pio::gpio::set_direction_native(args, objects)
            }
            ("picodroid/pio/Gpio", "setValue") => {
                crate::system::picodroid::pio::gpio::set_value_native(args, objects)
            }
            ("picodroid/pio/Gpio", "close") => Ok(None),
            ("picodroid/os/SystemClock", "sleep") => {
                crate::system::picodroid::os::system_clock::sleep(args)
            }
            _ => Err(JvmError::NoSuchMethod),
        }
    }
}
