use crate::framework::{
    array_heap::ArrayHeap,
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
        descriptor: &str,
        args: &[Value],
        strings: &mut StringTable,
        objects: &mut ObjectHeap,
        _arrays: &mut ArrayHeap,
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
            ("java/lang/Object", "<init>") => Ok(None),
            ("java/lang/StringBuilder", "<init>") => {
                objects.sb_clear();
                Ok(None)
            }
            ("java/lang/StringBuilder", "append") => {
                match args.get(1) {
                    Some(Value::Reference(idx)) => {
                        let s = strings.resolve(*idx).unwrap_or("");
                        objects.sb_append_bytes(s.as_bytes());
                    }
                    Some(Value::Int(n)) => {
                        if descriptor.starts_with("(C)") {
                            // append(char): emit the character, not its decimal value
                            let ch = (*n as u8).max(0x20); // replace non-printable with space
                            objects.sb_append_bytes(&[ch]);
                        } else {
                            objects.sb_append_int(*n);
                        }
                    }
                    _ => {}
                }
                Ok(args.first().copied().map(Some).unwrap_or(None))
            }
            ("java/lang/StringBuilder", "toString") => {
                let (ptr, len) = objects.sb_contents();
                // SAFETY: ptr points into ObjectHeap::sb_buf which lives for the
                // duration of the JVM. The dyn_slot in StringTable is overwritten on
                // the next toString() call, which is safe because Log.i (the only
                // consumer) copies the string before that happens.
                let str_ref =
                    unsafe { strings.intern_dyn(ptr, len) }.ok_or(JvmError::StackOverflow)?;
                Ok(Some(Value::Reference(str_ref)))
            }
            ("java/lang/String", "length") => {
                if let Some(Value::Reference(idx)) = args.first() {
                    let s = strings.resolve(*idx).unwrap_or("");
                    Ok(Some(Value::Int(s.len() as i32)))
                } else {
                    Err(JvmError::InvalidReference)
                }
            }
            ("java/lang/String", "charAt") => {
                if let (Some(Value::Reference(idx)), Some(Value::Int(i))) =
                    (args.first(), args.get(1))
                {
                    let s = strings.resolve(*idx).unwrap_or("");
                    let ch = s.as_bytes().get(*i as usize).copied().unwrap_or(0);
                    Ok(Some(Value::Int(ch as i32)))
                } else {
                    Err(JvmError::InvalidReference)
                }
            }
            _ => Err(JvmError::NoSuchMethod),
        }
    }
}
