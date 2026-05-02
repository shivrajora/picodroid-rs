// SPDX-License-Identifier: GPL-3.0-only
//! Pre-spawned FreeRTOS worker pool backing `Executors.backgroundExecutor()`.
//!
//! A fixed number of worker tasks (configured by `board.toml`'s optional
//! `[background_pool]` section; default 4 / BG tier priority 5 / 4 KiB stack)
//! park on a shared bounded `Queue<u16>`. Each worker owns its own
//! `pico_jvm::Jvm` instance with framework + application classes loaded,
//! mirroring the per-task JVM pattern used by `picodroid.concurrent.Thread`.
//!
//! `execute()` on the Java side is non-blocking: if the queue is saturated
//! the work is dropped with a `defmt::warn`. Blocking would risk UI-thread
//! deadlock when the main thread posts work into a full pool.
//!
//! Sim mode has no FreeRTOS; `submit()` delegates to the main-thread queue
//! so work still runs (serialised onto the UI thread, matching sim's
//! single-threaded guarantee) and executor-based apps are observable.

// Pool constants are consumed only by the device worker spawner; in sim the
// pool submits inline onto the main queue, so the symbols are unused there.
#[cfg_attr(feature = "sim", allow(dead_code))]
mod config {
    include!(concat!(env!("OUT_DIR"), "/background_pool_config.rs"));
}
#[cfg(all(not(feature = "sim"), feature = "family-rp"))]
use config::{POOL_PRIORITY, POOL_QUEUE_DEPTH, POOL_STACK_BYTES, POOL_THREADS};

#[cfg(all(not(feature = "sim"), feature = "family-rp"))]
mod device {
    use core::cell::UnsafeCell;

    use freertos_rust::{Duration, Queue, Task, TaskPriority};

    use super::{POOL_QUEUE_DEPTH, POOL_STACK_BYTES, POOL_THREADS};
    // POOL_PRIORITY lives one level up; referenced via super::super to avoid
    // shadowing the re-export inside this nested module.
    use super::POOL_PRIORITY;

    struct QueueCell(UnsafeCell<Option<Queue<u32>>>);
    // SAFETY: installed exactly once in `spawn()` pre-scheduler.
    unsafe impl Sync for QueueCell {}

    static WORK_QUEUE: QueueCell = QueueCell(UnsafeCell::new(None));

    fn queue() -> &'static Queue<u32> {
        unsafe {
            (*WORK_QUEUE.0.get())
                .as_ref()
                .expect("background_pool not initialised")
        }
    }

    /// Spawn the configured number of worker tasks. Must be called after the
    /// embedded framework + APK are available (so `app::load_classes`
    /// succeeds inside each worker) and before `FreeRtosUtils::start_scheduler`.
    pub fn spawn() {
        // SAFETY: pre-scheduler single-threaded.
        unsafe {
            if (*WORK_QUEUE.0.get()).is_some() {
                return;
            }
            let q = Queue::<u32>::new(POOL_QUEUE_DEPTH as usize).expect("bg pool queue alloc");
            *WORK_QUEUE.0.get() = Some(q);
        }

        for i in 0..POOL_THREADS {
            let _ = Task::new()
                .name("jvm-bg")
                // FreeRTOS stack size is counted in words (StackType_t = u32),
                // not bytes; freertos-rust accepts a u16 word count.
                .stack_size((POOL_STACK_BYTES / core::mem::size_of::<u32>() as u32) as u16)
                .priority(TaskPriority(POOL_PRIORITY))
                .core_affinity(0b01)
                .start(move |_| worker_loop(i));
        }
    }

    fn worker_loop(worker_id: u32) {
        // Jvm construction is deferred until the first queue receive so that
        // `crate::app::register_class_loader` (called from `run_jvm_with`) is
        // guaranteed to have run — no Runnable can reach the queue before
        // Java code runs, and only Java code submits work.
        let mut jvm: Option<pico_jvm::Jvm> = None;
        let mut handler = crate::system::native_handler::PicodroidNativeHandler::new();

        loop {
            let word = match queue().receive(Duration::infinite()) {
                Ok(w) => w,
                Err(_) => continue,
            };
            let obj_ref = (word & 0xFFFF) as u16;

            if jvm.is_none() {
                let mut j = pico_jvm::Jvm::new();
                if let Err(e) = crate::app::load_classes(&mut j) {
                    defmt::error!(
                        "background_pool[{}]: class load failed: {}",
                        worker_id,
                        defmt::Display2Format(&e)
                    );
                    continue;
                }
                jvm = Some(j);
            }
            let j = jvm.as_mut().unwrap();
            let heap = crate::app::shared_heap();

            // Route through the `Executors.dispatchRunnable` bytecode bridge
            // (see the matching lifecycle.rs call) so lambda proxies resolve
            // via invokeinterface rather than being dropped on the abstract
            // Runnable.run interface method.
            if let Err(e) = j.invoke_static_with_args(
                crate::shrink_names::shrink_class("picodroid/concurrent/Executors"),
                "dispatchRunnable",
                &[pico_jvm::types::Value::ObjectRef(obj_ref)],
                heap,
                &mut handler,
            ) {
                defmt::error!(
                    "background_pool[{}]: Runnable.run() failed: {}",
                    worker_id,
                    defmt::Display2Format(&e)
                );
            }
        }
    }

    /// Non-blocking submit. Returns `true` if enqueued, `false` if dropped
    /// (queue full). On drop the caller is expected to log a warning.
    pub fn submit(obj_ref: u16) -> bool {
        queue().send(obj_ref as u32, Duration::zero()).is_ok()
    }
}

#[cfg(feature = "sim")]
mod sim {
    /// Sim mode spawn is a no-op — there is no FreeRTOS scheduler, and the
    /// sim is single-threaded. Only called via device boot; kept here so
    /// `pub use` below can re-export a symbol under either cfg without
    /// arms diverging.
    #[allow(dead_code)]
    pub fn spawn() {}

    /// Delegate to the main-thread queue so the Runnable runs on the next
    /// drain pass. Preserves the Executor contract (work eventually runs)
    /// without needing a second Jvm instance.
    pub fn submit(obj_ref: u16) -> bool {
        super::super::main_queue::enqueue_runnable(obj_ref)
    }
}

#[cfg(all(not(feature = "sim"), feature = "family-rp"))]
pub use device::{spawn, submit};
#[cfg(feature = "sim")]
pub use sim::submit;
// ESP (single-threaded stub): background tasks are silently dropped in M1.
#[cfg(all(not(feature = "sim"), feature = "family-esp"))]
pub fn submit(_obj_ref: u16) -> bool {
    false
}
#[cfg(all(not(feature = "sim"), feature = "family-esp"))]
pub fn spawn() {}
