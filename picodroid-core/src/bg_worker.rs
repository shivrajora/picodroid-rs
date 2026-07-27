// SPDX-License-Identifier: GPL-3.0-only
//! Body of a background-pool worker task.
//!
//! [`crate::executors::background_pool`] owns the work queue and spawns the
//! workers; this is the loop each one runs.
//!
//! It lived in the platform crate until the class loader and shared heap
//! moved to [`crate::boot`]. Keeping it there afterwards would have been
//! expensive in a non-obvious way: it is the only other place that drives a
//! `Jvm`, so the interpreter was monomorphised into both crates —
//! `pico_jvm::interpreter::execute` alone appeared twice at 12.7 KB a copy,
//! ~38 KB across 23 symbols, which overflowed the RP2040 flash ceiling.
//! One JVM-driving crate means one instantiation.
//!
//! The split is deliberately "install a loop", not "install a per-item
//! callback": each worker builds its `Jvm` once and reuses it for every
//! subsequent Runnable. A per-item callback would have rebuilt and reloaded
//! classes on each dispatch, which is the same code but roughly two orders
//! of magnitude slower.

use crate::executors::background_pool;

/// Install [`worker_body`] as the pool's worker loop. Call before
/// `background_pool::spawn()`.
pub fn install() {
    background_pool::set_worker_body(worker_body);
}

fn worker_body(worker_id: u32) {
    // `Jvm` construction is deferred until the first work item so that
    // `boot::register_class_loader` (called from `run_app`) is
    // guaranteed to have run: no Runnable can reach the queue before Java
    // code runs, and only Java code submits work.
    let mut jvm: Option<pico_jvm::Jvm> = None;
    let mut handler = crate::native_handler::PicodroidNativeHandler::new();

    loop {
        let Some(obj_ref) = background_pool::recv_work() else {
            // Pool not spawned — nothing to drain.
            continue;
        };

        if jvm.is_none() {
            let mut j = pico_jvm::Jvm::new();
            if let Err(e) = crate::boot::load_classes(&mut j) {
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
        let heap = crate::boot::shared_heap();

        // Route through the `Executors.dispatchRunnable` bytecode bridge
        // (see the matching lifecycle.rs call) so lambda proxies resolve via
        // invokeinterface rather than being dropped on the abstract
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
