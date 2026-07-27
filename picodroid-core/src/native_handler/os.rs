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
        ("picodroid/os/SystemClock", "sleep") => Some(crate::os::system_clock::sleep(ctx.args)),
        ("picodroid/os/SystemClock", "elapsedRealtimeNanos") => {
            Some(crate::os::system_clock::elapsed_realtime_nanos())
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

                    // Field 0 = target (Runnable), field 1 = priority (int).
                    let android_priority = match ctx.objects.get_field(*thread_idx, 1) {
                        Some(Value::Int(p)) => p,
                        _ => 5, // Thread.NORM_PRIORITY fallback
                    };
                    let spec = crate::rtos::TaskSpec {
                        // The class name rides along as the task name so the
                        // platform can identify the Runnable — the simulator
                        // prints it when declining, and it shows up in the
                        // debug bridge's task list on device.
                        name: class_name,
                        kind: crate::rtos::TaskKind::JvmChild,
                        priority: crate::task_priority::android_to_freertos_priority(
                            android_priority,
                        ),
                        stack_bytes: None, // platform's JvmChild default
                    };

                    // One call for every target. Core-0 pinning and the debug
                    // bridge's child-task bookkeeping live in the platform's
                    // Rtos::spawn; the simulator declines this task kind
                    // outright (host threads cannot honour the interpreter's
                    // single-core heap guarantee) and reports it there.
                    crate::rtos::spawn(
                        &spec,
                        alloc::boxed::Box::new(move || {
                            let mut jvm = pico_jvm::Jvm::new();
                            // Don't unwrap: a class-load failure inside a child
                            // task would `bkpt`-halt the whole MCU under
                            // panic-probe and freeze USB CDC, leaving pdb unable
                            // to PING the device. Log and bail instead so
                            // jvm_task and PDB stay alive.
                            if let Err(e) = crate::boot::load_classes(&mut jvm) {
                                crate::pd_error!(
                                    "Thread.start: child-task class load failed for {}: {}",
                                    class_name,
                                    defmt::Display2Format(&e)
                                );
                                return;
                            }
                            let heap = crate::boot::shared_heap();
                            let mut handler = super::PicodroidNativeHandler::new();
                            if let Err(e) = jvm.invoke_instance(
                                class_name,
                                "run",
                                runnable_obj_idx,
                                heap,
                                &mut handler,
                            ) {
                                crate::pd_error!(
                                    "Thread.start: child-task {}.run() failed: {}",
                                    class_name,
                                    defmt::Display2Format(&e)
                                );
                            }
                        }),
                    );
                }
            }
            Some(Ok(None))
        }
        _ => None,
    }
}
