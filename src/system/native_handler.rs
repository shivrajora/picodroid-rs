use picodroid_jvm::{
    types::{JvmError, Value},
    NativeContext, NativeMethodHandler,
};

pub struct PicodroidNativeHandler;

impl NativeMethodHandler for PicodroidNativeHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        match (class_name, method_name) {
            ("picodroid/util/Log", "i") => Some(
                crate::system::picodroid::util::log::log_i(ctx.args, ctx.strings).map(|_| None),
            ),
            ("picodroid/pio/PeripheralManager", "getInstance") => {
                Some(crate::system::picodroid::pio::peripheral_manager::get_instance(ctx.objects))
            }
            ("picodroid/pio/PeripheralManager", "openGpio") => Some(
                crate::system::picodroid::pio::peripheral_manager::open_gpio(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ),
            ),
            ("picodroid/pio/PeripheralManager", "openUartDevice") => Some(
                crate::system::picodroid::pio::peripheral_manager::open_uart(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ),
            ),
            ("picodroid/pio/UartDevice", "setBaudrate") => Some(
                crate::system::picodroid::pio::uart::set_baudrate_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/UartDevice", "setDataSize") => Some(
                crate::system::picodroid::pio::uart::set_data_size_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/UartDevice", "setParity") => Some(
                crate::system::picodroid::pio::uart::set_parity_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/UartDevice", "setStopBits") => Some(
                crate::system::picodroid::pio::uart::set_stop_bits_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/UartDevice", "setHardwareFlowControl") => Some(
                crate::system::picodroid::pio::uart::set_hw_flow_ctrl_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/UartDevice", "writeByte") => Some(
                crate::system::picodroid::pio::uart::write_byte_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/UartDevice", "readByte") => Some(
                crate::system::picodroid::pio::uart::read_byte_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/UartDevice", "close") => Some(Ok(None)),
            ("picodroid/pio/Gpio", "setDirection") => Some(
                crate::system::picodroid::pio::gpio::set_direction_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/Gpio", "setValue") => Some(
                crate::system::picodroid::pio::gpio::set_value_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/Gpio", "close") => Some(Ok(None)),
            ("picodroid/os/SystemClock", "sleep") => {
                Some(crate::system::picodroid::os::system_clock::sleep(ctx.args))
            }
            ("picodroid/concurrent/Thread", "start") => {
                if let Some(Value::ObjectRef(thread_idx)) = ctx.args.first() {
                    if let Some(Value::ObjectRef(runnable_obj_idx)) =
                        ctx.objects.get_field(*thread_idx, 0)
                    {
                        let class_name: &'static str = ctx
                            .objects
                            .class_name(runnable_obj_idx)
                            .ok_or(JvmError::InvalidReference)
                            .ok()?;
                        freertos_rust::Task::new()
                            .name("jvm-t")
                            .stack_size(4096)
                            .start(move |_| {
                                let mut jvm = picodroid_jvm::Jvm::new();
                                crate::app::load_classes(&mut jvm).unwrap();
                                let heap = crate::app::shared_heap();
                                let mut handler = PicodroidNativeHandler;
                                jvm.invoke_instance(
                                    class_name,
                                    "run",
                                    runnable_obj_idx,
                                    heap,
                                    &mut handler,
                                )
                                .ok();
                                loop {
                                    freertos_rust::CurrentTask::delay(freertos_rust::Duration::ms(
                                        60_000,
                                    ));
                                }
                            })
                            .unwrap();
                    }
                }
                Some(Ok(None))
            }
            _ => None,
        }
    }
}
