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
//! Workers execute against the shared class set (`boot::shared_jvm`) —
//! historically each worker built a private `Jvm` and reloaded every class
//! (a permanent per-worker duplicate of the parsed metadata; see
//! docs/mem-session-2026-08.md for the measured cost).

use crate::executors::background_pool;

/// Install [`worker_body`] as the pool's worker loop. Call before
/// `background_pool::spawn()`.
pub fn install() {
    background_pool::set_worker_body(worker_body);
}

fn worker_body(worker_id: u32) {
    let mut handler = crate::native_handler::PicodroidNativeHandler::new();
    // Cross-executor GC root visibility for this worker's pending state.
    let _handler_roots = crate::native_handler::HandlerRootGuard::new(&handler);

    loop {
        let Some(obj_ref) = background_pool::recv_work() else {
            // Pool not spawned — nothing to drain.
            continue;
        };

        // Published by `run_app` before any Java code runs, and only Java
        // code submits work — so a miss means a torn-down app, not a race.
        let Some(j) = crate::boot::shared_jvm() else {
            crate::pd_error!(
                "background_pool[{}]: no shared class set — dropping work item",
                worker_id
            );
            continue;
        };
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
            crate::pd_error!(
                "background_pool[{}]: Runnable.run() failed: {}",
                worker_id,
                defmt::Display2Format(&e)
            );
        }
    }
}
