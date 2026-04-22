//! Native dispatch for `picodroid.concurrent.Executors` and the built-in
//! `MainExecutor` / `BackgroundExecutor` instances.

use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

use crate::system::executors::{background_pool, main_queue};

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        // Factory methods return a fresh executor instance each call. The
        // Rust-side queues are static, so identity of the returned object
        // does not matter — every instance routes to the same inbox/pool.
        ("picodroid/concurrent/Executors", "mainExecutor") => {
            let exec_class = crate::shrink_names::shrink_class("picodroid/concurrent/MainExecutor");
            match ctx.objects.alloc(exec_class) {
                Some(obj) => Some(Ok(Some(Value::ObjectRef(obj)))),
                None => Some(Err(JvmError::StackOverflow)),
            }
        }
        ("picodroid/concurrent/Executors", "backgroundExecutor") => {
            let exec_class =
                crate::shrink_names::shrink_class("picodroid/concurrent/BackgroundExecutor");
            match ctx.objects.alloc(exec_class) {
                Some(obj) => Some(Ok(Some(Value::ObjectRef(obj)))),
                None => Some(Err(JvmError::StackOverflow)),
            }
        }

        // execute(Runnable r): args[0] = this, args[1] = Runnable ObjectRef.
        ("picodroid/concurrent/MainExecutor", "execute") => {
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
        ("picodroid/concurrent/BackgroundExecutor", "execute") => {
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
        _ => None,
    }
}
