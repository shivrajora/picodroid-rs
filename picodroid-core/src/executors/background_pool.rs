// SPDX-License-Identifier: GPL-3.0-only
//! Fixed-size worker pool backing `picodroid.concurrent.Executors`.
//!
//! Workers block on a shared queue of `Runnable` obj_refs. Each owns its own
//! `Jvm` (lazily built on first work item) so app code can run off the UI
//! thread without contending for the interpreter.
//!
//! # Where the worker body lives
//!
//! The loop that builds a `Jvm`, loads classes and invokes
//! `Executors.dispatchRunnable` still lives in the platform crate, because it
//! reaches into the app's class loader and shared heap. This module owns the
//! queue and the spawning; the platform installs its loop via
//! [`set_worker_body`] and pulls work with [`recv_work`]. That keeps the
//! per-worker `Jvm` cached across items, which a per-item callback would
//! have quietly destroyed.
//!
//! # No simulator special case
//!
//! There used to be a `mod sim` whose `submit` forwarded to the main queue
//! and whose `spawn` was a no-op. That distinction is now implicit: the
//! simulator never calls [`spawn`], so no queue exists, and [`submit`] falls
//! back to the main queue on its own. Same behaviour, one code path — work
//! still runs, serialised onto the UI thread, matching the simulator's
//! single-threaded guarantee.

use alloc::boxed::Box;
use core::cell::Cell;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::board_cfg::background_pool as config;
use crate::rtos::{self, RawQueue, TaskKind, TaskSpec, Timeout};

/// Work queue handle; `0` means the pool was never spawned.
static QUEUE: AtomicUsize = AtomicUsize::new(0);

struct WorkerBodyCell(Cell<Option<fn(u32)>>);
// SAFETY: written once during boot, before any worker exists, and read-only
// afterwards — the same single-writer discipline as the GC root registry.
unsafe impl Sync for WorkerBodyCell {}

/// Platform-installed worker loop. `None` means not installed, in which case
/// [`spawn`] does nothing.
static WORKER_BODY: WorkerBodyCell = WorkerBodyCell(Cell::new(None));

fn queue() -> Option<RawQueue> {
    match QUEUE.load(Ordering::Acquire) {
        0 => None,
        q => Some(q),
    }
}

/// Install the worker loop. Call before [`spawn`].
pub fn set_worker_body(body: fn(u32)) {
    WORKER_BODY.0.set(Some(body));
}

/// Spawn the configured number of workers.
///
/// Must run after the framework and APK are available (so the worker body's
/// class load succeeds) and, on an RTOS that requires it, before the
/// scheduler starts. Idempotent, and a no-op if no worker body was
/// installed.
pub fn spawn() {
    if queue().is_some() {
        return;
    }
    let Some(body) = WORKER_BODY.0.get() else {
        return;
    };
    let q = rtos::queue_create(config::POOL_QUEUE_DEPTH as usize);
    if q == 0 {
        return;
    }
    QUEUE.store(q, Ordering::Release);

    for i in 0..config::POOL_THREADS {
        let spec = TaskSpec {
            name: "jvm-bg",
            kind: TaskKind::BgWorker,
            priority: config::POOL_PRIORITY,
            stack_bytes: Some(config::POOL_STACK_BYTES),
        };
        rtos::spawn(&spec, Box::new(move || body(i)));
    }
}

/// Block until a `Runnable` obj_ref is available. Called from the platform's
/// worker body. `None` means the pool was never spawned, which a worker
/// should treat as "retry" rather than "exit".
pub fn recv_work() -> Option<u16> {
    let q = queue()?;
    rtos::queue_recv(q, Timeout::Forever).map(|word| (word & 0xFFFF) as u16)
}

/// Non-blocking submit. Returns `true` if the work was accepted, `false` if
/// dropped because the queue is full; the caller is expected to log on drop.
///
/// With no pool spawned (the simulator), this forwards to the main queue so
/// the Runnable still runs on the next drain pass — preserving the Executor
/// contract that submitted work eventually executes.
pub fn submit(obj_ref: u16) -> bool {
    match queue() {
        Some(q) => rtos::queue_send(q, obj_ref as u32, Timeout::None),
        None => super::main_queue::enqueue_runnable(obj_ref),
    }
}
