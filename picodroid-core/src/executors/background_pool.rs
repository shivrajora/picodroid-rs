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

/// Shadow of every Runnable obj_ref in the pool queue — the GC-root mirror
/// of the RTOS queue, same rationale and locking as
/// `main_queue::PENDING_RUNNABLES` (bugbash F2).
const SHADOW_CAP: usize = config::POOL_QUEUE_DEPTH as usize;
struct ShadowCell(Cell<[u16; SHADOW_CAP]>, Cell<usize>);
unsafe impl Sync for ShadowCell {}
static PENDING_RUNNABLES: ShadowCell = ShadowCell(Cell::new([0; SHADOW_CAP]), Cell::new(0));

fn shadow_push(r: u16) {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let mut arr = PENDING_RUNNABLES.0.get();
    let len = PENDING_RUNNABLES.1.get();
    if len < SHADOW_CAP {
        arr[len] = r;
        PENDING_RUNNABLES.0.set(arr);
        PENDING_RUNNABLES.1.set(len + 1);
    }
}

fn shadow_remove(r: u16) {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let mut arr = PENDING_RUNNABLES.0.get();
    let len = PENDING_RUNNABLES.1.get();
    if let Some(pos) = arr[..len].iter().position(|&x| x == r) {
        arr.copy_within(pos + 1..len, pos);
        arr[len - 1] = 0;
        PENDING_RUNNABLES.0.set(arr);
        PENDING_RUNNABLES.1.set(len - 1);
    }
}

/// GC roots: every Runnable still queued for the pool.
pub fn visit_pending_runnable_roots(visit: &mut dyn FnMut(u16)) {
    let arr = PENDING_RUNNABLES.0.get();
    let len = PENDING_RUNNABLES.1.get();
    for &r in &arr[..len] {
        visit(r);
    }
}

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
    let r = rtos::queue_recv(q, Timeout::Forever).map(|word| (word & 0xFFFF) as u16);
    if let Some(r) = r {
        shadow_remove(r);
    }
    r
}

/// Non-blocking submit. Returns `true` if the work was accepted, `false` if
/// dropped because the queue is full; the caller is expected to log on drop.
///
/// With no pool spawned (the simulator), this forwards to the main queue so
/// the Runnable still runs on the next drain pass — preserving the Executor
/// contract that submitted work eventually executes.
pub fn submit(obj_ref: u16) -> bool {
    match queue() {
        Some(q) => {
            let sent = rtos::queue_send(q, obj_ref as u32, Timeout::None);
            if sent {
                shadow_push(obj_ref);
            }
            sent
        }
        None => super::main_queue::enqueue_runnable(obj_ref),
    }
}

/// Discard every queued Runnable. Called on a PDB app reload next to the
/// heap reset: the queue words are object indices into the *previous* app's
/// heap, and a worker draining them against the new heap would dispatch
/// `Executors.dispatchRunnable` on whatever now lives at that index.
/// Returns the number dropped.
pub fn drain() -> usize {
    let Some(q) = queue() else {
        return 0;
    };
    let mut n = 0;
    while let Some(word) = rtos::queue_recv(q, Timeout::None) {
        shadow_remove((word & 0xFFFF) as u16);
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_discards_queued_runnables() {
        // No queue yet: submit falls through to the main queue, drain is a no-op.
        assert_eq!(drain(), 0);
        let q = rtos::queue_create(4);
        assert_ne!(q, 0);
        QUEUE.store(q, Ordering::Release);
        assert!(submit(7));
        assert!(submit(8));
        assert_eq!(drain(), 2);
        assert_eq!(recv_nonblocking(), None);
        QUEUE.store(0, Ordering::Release);
    }

    fn recv_nonblocking() -> Option<u32> {
        rtos::queue_recv(queue().unwrap(), Timeout::None)
    }
}
