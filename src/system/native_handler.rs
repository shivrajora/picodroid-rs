use pico_jvm::{
    types::{JvmError, Value},
    NativeContext, NativeMethodHandler,
};

pub struct PicodroidNativeHandler {
    gc_time_ns: u64,
    gc_count: u32,
    gc_freed: u32,
}

impl PicodroidNativeHandler {
    pub fn new() -> Self {
        Self {
            gc_time_ns: 0,
            gc_count: 0,
            gc_freed: 0,
        }
    }
}

impl NativeMethodHandler for PicodroidNativeHandler {
    fn clock_nanos(&self) -> u64 {
        crate::hal::system_clock::elapsed_realtime_nanos() as u64
    }

    fn report_gc(&mut self, time_ns: u64, freed: usize) {
        self.gc_time_ns += time_ns;
        self.gc_count += 1;
        self.gc_freed += freed as u32;
    }

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
            ("picodroid/pio/PeripheralManager", "openI2cDevice") => {
                Some(crate::system::picodroid::pio::peripheral_manager::open_i2c(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ))
            }
            ("picodroid/pio/PeripheralManager", "openAdcPin") => {
                Some(crate::system::picodroid::pio::peripheral_manager::open_adc(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ))
            }
            ("picodroid/pio/Adc", "readValue") => Some(
                crate::system::picodroid::pio::adc::read_value_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/Adc", "close") => Some(Ok(None)),
            ("picodroid/pio/I2cDevice", "setSpeed") => Some(
                crate::system::picodroid::pio::i2c::set_speed_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/I2cDevice", "write") => Some(
                crate::system::picodroid::pio::i2c::write_native(ctx.args, ctx.objects, ctx.arrays),
            ),
            ("picodroid/pio/I2cDevice", "read") => Some(
                crate::system::picodroid::pio::i2c::read_native(ctx.args, ctx.objects, ctx.arrays),
            ),
            ("picodroid/pio/I2cDevice", "close") => Some(Ok(None)),
            ("picodroid/pio/PeripheralManager", "openPwm") => {
                Some(crate::system::picodroid::pio::peripheral_manager::open_pwm(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ))
            }
            ("picodroid/pio/Pwm", "setEnabled") => Some(
                crate::system::picodroid::pio::pwm::set_enabled_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/Pwm", "setPwmDutyCycle") => Some(
                crate::system::picodroid::pio::pwm::set_duty_cycle_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/Pwm", "setPwmFrequencyHz") => Some(
                crate::system::picodroid::pio::pwm::set_frequency_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/Pwm", "close") => Some(Ok(None)),
            ("picodroid/pio/PeripheralManager", "openSpiDevice") => {
                Some(crate::system::picodroid::pio::peripheral_manager::open_spi(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ))
            }
            ("picodroid/pio/SpiDevice", "setFrequency") => Some(
                crate::system::picodroid::pio::spi::set_frequency_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/SpiDevice", "setMode") => Some(
                crate::system::picodroid::pio::spi::set_mode_native(ctx.args, ctx.objects),
            ),
            ("picodroid/pio/SpiDevice", "transfer") => {
                Some(crate::system::picodroid::pio::spi::transfer_native(
                    ctx.args,
                    ctx.objects,
                    ctx.arrays,
                ))
            }
            ("picodroid/pio/SpiDevice", "write") => Some(
                crate::system::picodroid::pio::spi::write_native(ctx.args, ctx.objects, ctx.arrays),
            ),
            ("picodroid/pio/SpiDevice", "close") => Some(Ok(None)),
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
            ("picodroid/os/SystemClock", "elapsedRealtimeNanos") => {
                Some(crate::system::picodroid::os::system_clock::elapsed_realtime_nanos())
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

                        #[cfg(not(feature = "sim"))]
                        {
                            // Field 0 = target (Runnable), field 1 = priority (int).
                            let android_priority = match ctx.objects.get_field(*thread_idx, 1) {
                                Some(Value::Int(p)) => p,
                                _ => 5, // Thread.NORM_PRIORITY fallback
                            };
                            let freertos_prio = crate::task_priority::android_to_freertos_priority(
                                android_priority,
                            );
                            let child_task = freertos_rust::Task::new()
                                .name("jvm-t")
                                .stack_size(4096)
                                .priority(freertos_rust::TaskPriority(freertos_prio))
                                .core_affinity(0b01) // core 0 only
                                .start(move |_| {
                                    let mut jvm = pico_jvm::Jvm::new();
                                    crate::app::load_classes(&mut jvm).unwrap();
                                    let heap = crate::app::shared_heap();
                                    let mut handler = PicodroidNativeHandler::new();
                                    jvm.invoke_instance(
                                        class_name,
                                        "run",
                                        runnable_obj_idx,
                                        heap,
                                        &mut handler,
                                    )
                                    .ok();
                                    // Deregister before returning so jvm_task's wait loop unblocks.
                                    // The spawn_inner trampoline calls vTaskDelete(NULL) after
                                    // this closure returns, reclaiming the stack and TCB.
                                    crate::pdb::pending::deregister_child_task(
                                        freertos_rust::Task::current().unwrap().raw_handle(),
                                    );
                                })
                                .unwrap();
                            crate::pdb::pending::register_child_task(child_task);
                        }

                        #[cfg(feature = "sim")]
                        {
                            // Threading not supported in sim mode; log and skip.
                            println!(
                                "[sim] Thread.start() for '{}' skipped (sim is single-threaded)",
                                class_name
                            );
                        }
                    }
                }
                Some(Ok(None))
            }
            ("picodroid/os/Runtime", "gcTimeNanos") => {
                Some(Ok(Some(Value::Long(self.gc_time_ns as i64))))
            }
            ("picodroid/os/Runtime", "gcCount") => Some(Ok(Some(Value::Int(self.gc_count as i32)))),
            ("picodroid/os/Runtime", "gcFreed") => Some(Ok(Some(Value::Int(self.gc_freed as i32)))),
            ("picodroid/os/Runtime", "resetGcStats") => {
                self.gc_time_ns = 0;
                self.gc_count = 0;
                self.gc_freed = 0;
                Some(Ok(None))
            }
            _ => None,
        }
    }

    #[cfg(not(any(test, feature = "sim")))]
    fn interrupted(&self) -> bool {
        crate::pdb::pending::STOP_JVM.load(core::sync::atomic::Ordering::Relaxed)
    }
}
