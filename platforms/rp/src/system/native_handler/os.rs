// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        ("picodroid/os/SystemClock", "sleep") => {
            Some(crate::system::picodroid::os::system_clock::sleep(ctx.args))
        }
        ("picodroid/os/SystemClock", "elapsedRealtimeNanos") => {
            Some(crate::system::picodroid::os::system_clock::elapsed_realtime_nanos())
        }
        ("java/lang/System", "currentTimeMillis") => {
            let nanos = crate::hal::system_clock::elapsed_realtime_nanos();
            Some(Ok(Some(Value::Long(nanos / 1_000_000))))
        }
        ("picodroid/content/pm/PackageManager", "hasSystemFeature") => {
            // args[0] = this, args[1] = feature name String
            let supported = match ctx.args.get(1) {
                Some(Value::Reference(idx)) => match ctx.strings.resolve(*idx) {
                    // FEATURE_WIFI: board has a wireless driver compiled in.
                    Some("picodroid.hardware.wifi") => cfg!(has_network),
                    _ => false,
                },
                _ => false,
            };
            Some(Ok(Some(Value::Int(supported as i32))))
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

                    #[cfg(all(not(feature = "sim"), feature = "family-rp"))]
                    {
                        // Field 0 = target (Runnable), field 1 = priority (int).
                        let android_priority = match ctx.objects.get_field(*thread_idx, 1) {
                            Some(Value::Int(p)) => p,
                            _ => 5, // Thread.NORM_PRIORITY fallback
                        };
                        let freertos_prio =
                            crate::task_priority::android_to_freertos_priority(android_priority);
                        let task_result = freertos_rust::Task::new()
                            .name("jvm-t")
                            .stack_size(4096)
                            .priority(freertos_rust::TaskPriority(freertos_prio))
                            .core_affinity(0b01) // core 0 only
                            .start(move |_| {
                                let mut jvm = pico_jvm::Jvm::new();
                                // Don't unwrap: a class-load failure inside a child task
                                // would `bkpt`-halt the whole MCU under panic-probe and
                                // freeze USB CDC, leaving pdb unable to PING the device.
                                // Log and bail instead so jvm_task and PDB stay alive.
                                if let Err(e) = crate::app::load_classes(&mut jvm) {
                                    defmt::error!(
                                        "Thread.start: child-task class load failed for {}: {}",
                                        class_name,
                                        defmt::Display2Format(&e)
                                    );
                                } else {
                                    let heap = crate::app::shared_heap();
                                    let mut handler = super::PicodroidNativeHandler::new();
                                    if let Err(e) = jvm.invoke_instance(
                                        class_name,
                                        "run",
                                        runnable_obj_idx,
                                        heap,
                                        &mut handler,
                                    ) {
                                        defmt::error!(
                                            "Thread.start: child-task {}.run() failed: {}",
                                            class_name,
                                            defmt::Display2Format(&e)
                                        );
                                    }
                                }
                                // Deregister before returning so jvm_task's wait loop unblocks.
                                // The spawn_inner trampoline calls vTaskDelete(NULL) after
                                // this closure returns, reclaiming the stack and TCB.
                                if let Ok(t) = freertos_rust::Task::current() {
                                    crate::pdb::pending::deregister_child_task(t.raw_handle());
                                }
                            });
                        match task_result {
                            Ok(child_task) => {
                                crate::pdb::pending::register_child_task(child_task);
                            }
                            Err(e) => {
                                defmt::error!(
                                    "Thread.start: failed to spawn FreeRTOS task for {}: {:?}",
                                    class_name,
                                    defmt::Debug2Format(&e)
                                );
                            }
                        }
                    }

                    #[cfg(feature = "sim")]
                    {
                        // Threading is a no-op in the simulator — there's no
                        // FreeRTOS to host the task. Warn loudly (naming the
                        // Runnable) so a dev doesn't read the silence as "it
                        // ran": on device this spawns a real task and the
                        // Runnable executes. Erroring would misrepresent device
                        // behavior worse than a no-op, and running it
                        // synchronously here would invert concurrency ordering
                        // and can deadlock on the main queue — so: warn + skip.
                        eprintln!(
                            "[sim] Thread.start: {class_name}.run() will NOT run — \
                             threads are a no-op in the simulator (on device they \
                             run as a FreeRTOS task)"
                        );
                    }
                }
            }
            Some(Ok(None))
        }
        _ => None,
    }
}
