#[cfg(not(feature = "sim"))]
use pico_jvm::types::MonitorKey;
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

/// Returns `true` if `class_name` is `picodroid/view/View` or any of its subclasses.
/// Used by match guards so that inherited native methods (setSize, setPosition, …)
/// dispatch correctly when invokevirtual passes the runtime class name.
fn is_view(class_name: &str) -> bool {
    matches!(
        class_name,
        "picodroid/view/View"
            | "picodroid/widget/TextView"
            | "picodroid/widget/Button"
            | "picodroid/widget/LinearLayout"
            | "picodroid/widget/ProgressBar"
            | "picodroid/widget/Switch"
            | "picodroid/widget/ListView"
            | "picodroid/widget/ImageView"
    )
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
            // ── Display ───────────────────────────────────────────────────
            ("picodroid/graphics/Display", "getInstance") => Some(
                crate::system::picodroid::graphics::display::get_instance(ctx.objects),
            ),
            ("picodroid/graphics/Display", "setContentView") => Some(
                crate::system::picodroid::graphics::display::set_content_view(
                    ctx.args,
                    ctx.objects,
                ),
            ),
            ("picodroid/graphics/Display", "pollTouch") => Some(
                crate::system::picodroid::graphics::display::poll_touch(ctx.objects),
            ),
            ("picodroid/graphics/Display", "update") => {
                Some(crate::system::picodroid::graphics::display::update())
            }
            ("picodroid/graphics/Display", "calibrate") => {
                Some(crate::system::picodroid::graphics::display::calibrate())
            }

            // ── View (base class) ────────────────────────────────────────
            // invokevirtual passes the runtime class name, so inherited View
            // methods may arrive as any subclass name.
            (c, "setPosition") if is_view(c) => Some(
                crate::system::picodroid::graphics::view::set_position(ctx.args, ctx.objects),
            ),
            (c, "setSize") if is_view(c) => Some(
                crate::system::picodroid::graphics::view::set_size(ctx.args, ctx.objects),
            ),
            (c, "setBackgroundColor") if is_view(c) => Some(
                crate::system::picodroid::graphics::view::set_bg_color(ctx.args, ctx.objects),
            ),
            (c, "setVisibility") if is_view(c) => Some(
                crate::system::picodroid::graphics::view::set_visibility(ctx.args, ctx.objects),
            ),
            (c, "close") if is_view(c) => Some(crate::system::picodroid::graphics::view::close(
                ctx.args,
                ctx.objects,
            )),

            // ── TextView ─────────────────────────────────────────────────
            ("picodroid/widget/TextView", "nativeCreate") => {
                Some(crate::system::picodroid::graphics::widgets::text_view_native_create())
            }
            ("picodroid/widget/TextView", "setText") => Some(
                crate::system::picodroid::graphics::widgets::text_view_set_text(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ),
            ),
            ("picodroid/widget/TextView", "setTextColor") => Some(
                crate::system::picodroid::graphics::widgets::text_view_set_text_color(
                    ctx.args,
                    ctx.objects,
                ),
            ),

            // ── Button ───────────────────────────────────────────────────
            ("picodroid/widget/Button", "nativeCreate") => Some(
                crate::system::picodroid::graphics::widgets::button_native_create(
                    ctx.args,
                    ctx.strings,
                ),
            ),
            ("picodroid/widget/Button", "setText") => Some(
                crate::system::picodroid::graphics::widgets::button_set_text(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ),
            ),
            ("picodroid/widget/Button", "wasClicked") => Some(
                crate::system::picodroid::graphics::widgets::button_was_clicked(
                    ctx.args,
                    ctx.objects,
                ),
            ),

            // ── LinearLayout ─────────────────────────────────────────────
            ("picodroid/widget/LinearLayout", "nativeCreate") => {
                Some(crate::system::picodroid::graphics::widgets::linear_layout_native_create())
            }
            ("picodroid/widget/LinearLayout", "addView") => Some(
                crate::system::picodroid::graphics::widgets::linear_layout_add_view(
                    ctx.args,
                    ctx.objects,
                ),
            ),
            ("picodroid/widget/LinearLayout", "setOrientation") => Some(
                crate::system::picodroid::graphics::widgets::linear_layout_set_orientation(
                    ctx.args,
                    ctx.objects,
                ),
            ),

            // ── ProgressBar ──────────────────────────────────────────────
            ("picodroid/widget/ProgressBar", "nativeCreate") => {
                Some(crate::system::picodroid::graphics::widgets::progress_bar_native_create())
            }
            ("picodroid/widget/ProgressBar", "setProgress") => Some(
                crate::system::picodroid::graphics::widgets::progress_bar_set_progress(
                    ctx.args,
                    ctx.objects,
                ),
            ),

            // ── Switch ───────────────────────────────────────────────────
            ("picodroid/widget/Switch", "nativeCreate") => {
                Some(crate::system::picodroid::graphics::widgets::switch_native_create())
            }
            ("picodroid/widget/Switch", "isChecked") => Some(
                crate::system::picodroid::graphics::widgets::switch_is_checked(
                    ctx.args,
                    ctx.objects,
                ),
            ),
            ("picodroid/widget/Switch", "setChecked") => Some(
                crate::system::picodroid::graphics::widgets::switch_set_checked(
                    ctx.args,
                    ctx.objects,
                ),
            ),

            // ── ListView ─────────────────────────────────────────────────
            ("picodroid/widget/ListView", "nativeCreate") => {
                Some(crate::system::picodroid::graphics::widgets::list_view_native_create())
            }
            ("picodroid/widget/ListView", "addItem") => Some(
                crate::system::picodroid::graphics::widgets::list_view_add_item(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ),
            ),

            // ── ImageView ────────────────────────────────────────────────
            ("picodroid/widget/ImageView", "nativeCreate") => {
                Some(crate::system::picodroid::graphics::widgets::image_view_native_create())
            }
            ("picodroid/widget/ImageView", "setImageSource") => Some(
                crate::system::picodroid::graphics::widgets::image_view_set_src(
                    ctx.args,
                    ctx.strings,
                    ctx.objects,
                ),
            ),

            _ => None,
        }
    }

    #[cfg(not(any(test, feature = "sim")))]
    fn interrupted(&self) -> bool {
        crate::pdb::pending::STOP_JVM.load(core::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(not(feature = "sim"))]
    fn monitor_enter(&mut self, key: MonitorKey) -> Result<(), JvmError> {
        crate::system::monitor_store::enter(key)
    }

    #[cfg(not(feature = "sim"))]
    fn monitor_exit(&mut self, key: MonitorKey) -> Result<(), JvmError> {
        crate::system::monitor_store::exit(key)
    }

    #[cfg(not(feature = "sim"))]
    fn monitors_clear(&mut self) {
        crate::system::monitor_store::clear();
    }
}
