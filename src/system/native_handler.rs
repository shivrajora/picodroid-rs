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
            ("picodroid/pio/PeripheralManager", "openUartDevice") => {
                crate::system::picodroid::pio::peripheral_manager::open_uart(args, strings, objects)
            }
            ("picodroid/pio/UartDevice", "setBaudrate") => {
                crate::system::picodroid::pio::uart::set_baudrate_native(args, objects)
            }
            ("picodroid/pio/UartDevice", "setDataSize") => {
                crate::system::picodroid::pio::uart::set_data_size_native(args, objects)
            }
            ("picodroid/pio/UartDevice", "setParity") => {
                crate::system::picodroid::pio::uart::set_parity_native(args, objects)
            }
            ("picodroid/pio/UartDevice", "setStopBits") => {
                crate::system::picodroid::pio::uart::set_stop_bits_native(args, objects)
            }
            ("picodroid/pio/UartDevice", "setHardwareFlowControl") => {
                crate::system::picodroid::pio::uart::set_hw_flow_ctrl_native(args, objects)
            }
            ("picodroid/pio/UartDevice", "writeByte") => {
                crate::system::picodroid::pio::uart::write_byte_native(args, objects)
            }
            ("picodroid/pio/UartDevice", "readByte") => {
                crate::system::picodroid::pio::uart::read_byte_native(args, objects)
            }
            ("picodroid/pio/UartDevice", "close") => Ok(None),
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
