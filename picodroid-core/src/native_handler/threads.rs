// SPDX-License-Identifier: GPL-3.0-only
//! Native dispatch for `picodroid.concurrent.Thread` and for
//! `Object.wait`/`notify`/`notifyAll`.
//!
//! The registry behind every arm is [`crate::threads`]; monitor ownership
//! for `wait`/`notify` is [`crate::monitor_store`]'s.
//!
//! # `Object.wait` is matched by name, not by class
//!
//! javac writes the *static* receiver type into the method reference, so
//! `lock.wait()` on a `Foo` arrives here as `Foo.wait()V`, not
//! `java/lang/Object.wait()V`. These three methods are `final` on `Object`
//! — no class can redeclare them — so a `()V`/`(J)V` `wait`, `notify` or
//! `notifyAll` on an object receiver is unambiguous whatever class name
//! rides along. The arms therefore test the method name and descriptor
//! first. They are not rows in `method_tables.rs`: that table cross-checks
//! the SDK's own `native` declarations, and `java/lang/Object` is a builtin
//! with no class file.

use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, MonitorKey, Value},
    NativeContext,
};

use crate::threads::{self, Outcome};

/// Allocate a builtin exception by name with `msg`, the way the net stack's
/// `throw_named_exception` does — duplicated here because `crate::net` is
/// board-gated (`has_network`) and threads exist on every board.
fn throw_named_exception(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    class: &'static str,
    msg: &str,
) -> JvmError {
    match objects.alloc(class) {
        Some(idx) => {
            if let Some(midx) = strings.intern_dyn(msg.as_bytes()) {
                objects.register_exception_message(idx, midx);
            }
            JvmError::Exception(idx)
        }
        None => JvmError::StackOverflow,
    }
}

fn monitor_key(v: Option<&Value>) -> Option<MonitorKey> {
    match v {
        Some(Value::ObjectRef(i)) => Some(MonitorKey::Object(*i)),
        Some(Value::ArrayRef(i)) => Some(MonitorKey::Array(*i)),
        Some(Value::Reference(i)) => Some(MonitorKey::String(*i)),
        _ => None,
    }
}

/// A `long` millisecond argument as the registry's `Option<u32>` timeout:
/// `0` means forever (the Java convention for `wait`/`join`), negatives
/// are rejected by the Java side before they get here.
fn timeout_arg(v: Option<&Value>) -> Option<u32> {
    match v {
        Some(Value::Long(ms)) if *ms > 0 => Some((*ms).min(u32::MAX as i64) as u32),
        _ => None,
    }
}

fn interrupted(ctx: &mut NativeContext<'_>, what: &str) -> JvmError {
    throw_named_exception(
        ctx.objects,
        ctx.strings,
        "java/lang/InterruptedException",
        what,
    )
}

fn outcome_to_result(
    ctx: &mut NativeContext<'_>,
    o: Outcome,
    what: &str,
) -> Result<Option<Value>, JvmError> {
    match o {
        Outcome::Interrupted => Err(interrupted(ctx, what)),
        // Satisfied, TimedOut, Stopped: return normally — for Stopped the
        // interpreter's own stop check unwinds at the next safepoint.
        _ => Ok(None),
    }
}

fn object_wait(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let Some(key) = monitor_key(ctx.args.first()) else {
        return Err(JvmError::InvalidReference);
    };
    if !crate::monitor_store::held_by_current(key) {
        return Err(throw_named_exception(
            ctx.objects,
            ctx.strings,
            "java/lang/IllegalMonitorStateException",
            "current thread is not owner",
        ));
    }
    let timeout = timeout_arg(ctx.args.get(1));
    // Give the monitor up completely (recursion and all), wait, take it
    // back at the same depth — whatever woke us.
    let depth = crate::monitor_store::save_and_release(key)?;
    let outcome = threads::wait_current(key, timeout);
    crate::monitor_store::reacquire(key, depth)?;
    outcome_to_result(ctx, outcome, "wait")
}

fn object_notify(ctx: &mut NativeContext<'_>, all: bool) -> Result<Option<Value>, JvmError> {
    let Some(key) = monitor_key(ctx.args.first()) else {
        return Err(JvmError::InvalidReference);
    };
    if !crate::monitor_store::held_by_current(key) {
        return Err(throw_named_exception(
            ctx.objects,
            ctx.strings,
            "java/lang/IllegalMonitorStateException",
            "current thread is not owner",
        ));
    }
    threads::notify(key, all);
    Ok(None)
}

fn this_obj(ctx: &NativeContext<'_>) -> Result<u16, JvmError> {
    match ctx.args.first() {
        Some(Value::ObjectRef(o)) => Ok(*o),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `Thread.start0`: reserve the registry entry (rooting the `Thread`
/// object, and through its `target` field the Runnable, from before the
/// task exists), then spawn a task whose whole life is
/// `Thread.runWrapper(this)` — `run()` resolved by `invokevirtual`, so a
/// subclass override works, and the uncaught-exception path is Java.
fn thread_start0(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let this = this_obj(ctx)?;
    // The task name: the Thread's (sub)class — visible in the debug
    // bridge's task list. Canonical so it outlives this call.
    let task_name: &'static str = ctx.objects.class_name(this).unwrap_or("Thread");
    let Some(slot) = threads::reserve(this) else {
        crate::pd_warn!(
            "Thread.start: more than {} live threads, {} not started",
            threads::MAX_JAVA_THREADS,
            task_name
        );
        return Ok(Some(Value::Int(0)));
    };
    let spec = crate::rtos::TaskSpec {
        name: task_name,
        kind: crate::rtos::TaskKind::JvmChild,
        // Advisory `setPriority`: every interpreting task shares one tier.
        priority: crate::task_priority::PRIORITY_JVM_NORM,
        stack_bytes: None,
    };
    let spawned = crate::rtos::spawn(
        &spec,
        alloc::boxed::Box::new(move || {
            threads::bind_current(slot);
            // Shared class set and heap (`boot`); a missing set means the
            // app was torn down under us — log and leave, never halt (a
            // bkpt here would freeze USB CDC and lock the debug bridge out).
            if let Some(jvm) = crate::boot::shared_jvm() {
                let heap = crate::boot::shared_heap();
                let mut handler = super::PicodroidNativeHandler::new();
                let _handler_roots = super::HandlerRootGuard::new(&handler);
                let (class, method) =
                    crate::dispatch_sites::DISPATCH_SITES[crate::dispatch_sites::THREAD_RUN];
                if let Err(e) = jvm.invoke_static_with_args(
                    crate::shrink_names::shrink_class(class),
                    method,
                    &[Value::ObjectRef(this)],
                    heap,
                    &mut handler,
                ) {
                    // Java exceptions never get here (runWrapper catches
                    // Throwable); this is a debugger stop or an internal
                    // fault.
                    crate::pd_error!(
                        "Thread.start: {} left the interpreter: {}",
                        task_name,
                        defmt::Display2Format(&e)
                    );
                }
            } else {
                crate::pd_error!("Thread.start: no shared class set for {}", task_name);
            }
            // Normally already done by runWrapper's finally; on the error
            // paths above this is what releases the monitors it still
            // holds and wakes its joiners.
            threads::terminate(slot);
        }),
    );
    if !spawned {
        crate::pd_error!("Thread.start: task spawn failed for {}", task_name);
        threads::terminate(slot);
        return Ok(Some(Value::Int(0)));
    }
    Ok(Some(Value::Int(1)))
}

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    // Object.wait / notify / notifyAll — see the module docs for why these
    // are matched on name + descriptor and not on the class.
    if matches!(
        ctx.args.first(),
        Some(Value::ObjectRef(_) | Value::ArrayRef(_) | Value::Reference(_))
    ) {
        match (method_name, ctx.descriptor) {
            ("wait", "()V" | "(J)V") => return Some(object_wait(ctx)),
            ("notify", "()V") => return Some(object_notify(ctx, false)),
            ("notifyAll", "()V") => return Some(object_notify(ctx, true)),
            _ => {}
        }
    }

    if crate::shrink_names::unshrink_class(class_name) != "picodroid/concurrent/Thread" {
        return None;
    }
    Some(match method_name {
        "start0" => thread_start0(ctx),
        "current0" => Ok(Some(match threads::current_obj() {
            Some(o) => Value::ObjectRef(o),
            None => Value::Null,
        })),
        // 0 = the UI task ("main"), 1 = anything else (a worker names
        // itself "Thread-N" on the Java side).
        "currentKind0" => Ok(Some(Value::Int(if crate::ui_thread::is_ui_task() {
            0
        } else {
            1
        }))),
        "adopt0" => this_obj(ctx).map(|o| {
            threads::adopt_current(Some(o));
            None
        }),
        "sleep0" => {
            let ms = match ctx.args.first() {
                Some(Value::Long(ms)) => (*ms).clamp(0, u32::MAX as i64) as u32,
                _ => 0,
            };
            let o = threads::sleep_current(ms);
            outcome_to_result(ctx, o, "sleep interrupted")
        }
        "join0" => match this_obj(ctx) {
            Ok(target) => {
                let o = threads::join(target, timeout_arg(ctx.args.get(1)));
                outcome_to_result(ctx, o, "join interrupted")
            }
            Err(e) => Err(e),
        },
        "interrupt" => this_obj(ctx).map(|o| {
            threads::interrupt(o);
            None
        }),
        "isInterrupted" => {
            this_obj(ctx).map(|o| Some(Value::Int(threads::is_interrupted(o) as i32)))
        }
        "interrupted" => Ok(Some(Value::Int(threads::take_interrupted_current() as i32))),
        "isAlive" => this_obj(ctx).map(|o| Some(Value::Int(threads::is_alive(o) as i32))),
        // vTaskDelay(0) is FreeRTOS's "yield to an equal-priority task".
        "yield0" => {
            crate::rtos::delay_ms(0);
            Ok(None)
        }
        "exit0" => this_obj(ctx).map(|o| {
            threads::terminate_by_obj(o);
            None
        }),
        _ => return None,
    })
}
