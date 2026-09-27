// SPDX-License-Identifier: GPL-3.0-only
//! Native dispatch for `picodroid.concurrent.Executors`, the built-in
//! `MainExecutor` / `BackgroundExecutor` instances and the deadline table
//! behind `MainScheduledExecutor`.

use crate::shrink_names::m;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

use crate::executors::{background_pool, main_queue, scheduled};
use crate::shrink_names::c;

fn int_arg(ctx: &NativeContext<'_>, i: usize) -> i32 {
    match ctx.args.get(i) {
        Some(Value::Int(v)) => *v,
        _ => 0,
    }
}

/// A Java `long` of milliseconds as the table's `u32`: negative is now,
/// and anything past 24 days is clamped rather than wrapped.
fn ms_arg(ctx: &NativeContext<'_>, i: usize) -> u32 {
    match ctx.args.get(i) {
        Some(Value::Long(v)) => (*v).clamp(0, i32::MAX as i64) as u32,
        _ => 0,
    }
}

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match (class_name, method_name) {
        // Factory methods return a fresh executor instance each call. The
        // Rust-side queues are static, so identity of the returned object
        // does not matter — every instance routes to the same inbox/pool.
        (c::picodroid_concurrent_Executors, m::mainExecutor) => {
            let exec_class = c::picodroid_concurrent_MainExecutor;
            match ctx.objects.alloc(exec_class) {
                Some(obj) => Some(Ok(Some(Value::ObjectRef(obj)))),
                None => Some(Err(JvmError::StackOverflow)),
            }
        }
        (c::picodroid_concurrent_Executors, m::backgroundExecutor) => {
            let exec_class = c::picodroid_concurrent_BackgroundExecutor;
            match ctx.objects.alloc(exec_class) {
                Some(obj) => Some(Ok(Some(Value::ObjectRef(obj)))),
                None => Some(Err(JvmError::StackOverflow)),
            }
        }

        // execute(Runnable r): args[0] = this, args[1] = Runnable ObjectRef.
        (c::picodroid_concurrent_MainExecutor, m::execute) => {
            if let Some(Value::ObjectRef(runnable)) = ctx.args.get(1) {
                if !main_queue::enqueue_runnable(*runnable) {
                    #[cfg(not(feature = "sim"))]
                    defmt::warn!("MainExecutor.execute: queue full, dropped");
                    #[cfg(feature = "sim")]
                    eprintln!("[sim] MainExecutor.execute: queue full, dropped");
                }
            }
            Some(Ok(None))
        }
        (c::picodroid_concurrent_BackgroundExecutor, m::execute) => {
            if let Some(Value::ObjectRef(runnable)) = ctx.args.get(1) {
                if !background_pool::submit(*runnable) {
                    #[cfg(not(feature = "sim"))]
                    defmt::warn!("BackgroundExecutor.execute: queue full, dropped");
                    #[cfg(feature = "sim")]
                    eprintln!("[sim] BackgroundExecutor.execute: queue full, dropped");
                }
            }
            Some(Ok(None))
        }

        // The deadline table (`executors::scheduled`). Static natives:
        // args[0] is the first argument.
        (c::picodroid_concurrent_MainScheduledExecutor, m::schedule0) => {
            let Some(Value::ObjectRef(runnable)) = ctx.args.first() else {
                return Some(Ok(Some(Value::Int(-1))));
            };
            let id = scheduled::Kind::from_java(int_arg(ctx, 3))
                .and_then(|kind| {
                    scheduled::schedule(*runnable, ms_arg(ctx, 1), ms_arg(ctx, 2), kind)
                })
                .unwrap_or(-1);
            Some(Ok(Some(Value::Int(id))))
        }
        (c::picodroid_concurrent_MainScheduledExecutor, m::cancel0) => Some(Ok(Some(Value::Int(
            scheduled::cancel(int_arg(ctx, 0)) as i32,
        )))),
        (c::picodroid_concurrent_MainScheduledExecutor, m::completed0) => {
            scheduled::completed(int_arg(ctx, 0));
            Some(Ok(None))
        }
        (c::picodroid_concurrent_MainScheduledExecutor, m::delay0) => {
            let ms = scheduled::delay_ms(int_arg(ctx, 0)).unwrap_or(0);
            Some(Ok(Some(Value::Long(ms as i64))))
        }
        _ => None,
    }
}
