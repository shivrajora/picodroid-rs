use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match (class_name, method_name) {
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
                        let freertos_prio =
                            crate::task_priority::android_to_freertos_priority(android_priority);
                        let child_task = freertos_rust::Task::new()
                            .name("jvm-t")
                            .stack_size(4096)
                            .priority(freertos_rust::TaskPriority(freertos_prio))
                            .core_affinity(0b01) // core 0 only
                            .start(move |_| {
                                let mut jvm = pico_jvm::Jvm::new();
                                crate::app::load_classes(&mut jvm).unwrap();
                                let heap = crate::app::shared_heap();
                                let mut handler = super::PicodroidNativeHandler::new();
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
        _ => None,
    }
}
