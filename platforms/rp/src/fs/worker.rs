// SPDX-License-Identifier: GPL-3.0-only
//! FreeRTOS fs-worker task: serialises all runtime access to the mounted
//! LittleFS filesystem onto a single core-0-pinned task.
//!
//! Callers invoke [`submit`] with a closure.  The closure plus a slot for
//! its return value is placed on the caller's stack; a pointer to that
//! work item is pushed through a FreeRTOS queue and the caller blocks on a
//! task notification.  The worker dequeues, runs the closure against the
//! mounted filesystem via [`super::cell::with`], and notifies the caller.
//!
//! Because only the worker ever enters `cell::with`, same-core concurrency
//! between Java threads is resolved by construction: each request runs to
//! completion before the next one is dequeued, so littlefs internal state
//! is never mutated by two tasks at once.  Pinning the worker to core 0
//! keeps the existing XIP-disable window same-core as well.
//!
//! The flash primitives in `hal::flash` still disable interrupts during
//! erase/program — that stall happens inside this task rather than being
//! spread across arbitrary callers.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;

use freertos_rust::{CurrentTask, Duration, Queue, Task, TaskNotification, TaskPriority};
use littlefs_rust::Filesystem;

use crate::fs::FsStorage;
use crate::task_priority;

struct Work {
    /// Trampoline that calls the type-erased caller closure.
    run: unsafe fn(*mut ()),
    /// Pointer to a caller-allocated `Ctx<F, R>` on the caller's stack.
    ctx: *mut (),
    /// Handle of the task that submitted this work; notified on completion.
    waiter: Task,
}

struct QueueCell(UnsafeCell<Option<Queue<usize>>>);
// SAFETY: installed exactly once in `spawn()` pre-scheduler, then read-only.
unsafe impl Sync for QueueCell {}

static REQUESTS: QueueCell = QueueCell(UnsafeCell::new(None));

const QUEUE_CAPACITY: usize = 8;

fn queue() -> &'static Queue<usize> {
    // SAFETY: initialised in spawn() before any caller can reach here.
    unsafe {
        (*REQUESTS.0.get())
            .as_ref()
            .expect("fs worker queue not initialised")
    }
}

/// Spawn the fs-worker task.  Must be called after [`crate::fs::init`] and
/// before `FreeRtosUtils::start_scheduler`.
pub fn spawn() {
    let q = Queue::<usize>::new(QUEUE_CAPACITY).expect("fs worker queue");
    // SAFETY: pre-scheduler, single-threaded.
    unsafe { *REQUESTS.0.get() = Some(q) };

    Task::new()
        .name("fs")
        .stack_size(crate::boot_budget::FS_STACK_WORDS)
        .priority(TaskPriority(task_priority::PRIORITY_FS_WORKER))
        .core_affinity(0b01)
        .start(|_| loop {
            let ptr = match queue().receive(Duration::infinite()) {
                Ok(p) => p,
                Err(_) => continue,
            };
            // SAFETY: `ptr` points at a `Work` on the caller's stack; the
            // caller is blocked on `take_notification` until we notify, so
            // the stack frame is live for the duration of this call.
            let work = unsafe { &*(ptr as *const Work) };
            unsafe { (work.run)(work.ctx) };
            work.waiter.notify(TaskNotification::Increment);
        })
        .expect("fs worker task");
}

/// Submit `f` to the worker and block until it returns.
///
/// The closure is executed on the worker task's stack; its captured
/// references must remain valid for the duration of the call (they will be,
/// because this function blocks the caller until the worker signals done).
pub fn submit<F, R>(f: F) -> R
where
    F: FnOnce(&Filesystem<FsStorage>) -> R,
{
    struct Ctx<F, R> {
        f: Option<F>,
        out: MaybeUninit<R>,
    }

    unsafe fn trampoline<F, R>(ctx: *mut ())
    where
        F: FnOnce(&Filesystem<FsStorage>) -> R,
    {
        let ctx = &mut *(ctx as *mut Ctx<F, R>);
        let f = ctx.f.take().expect("fs trampoline called twice");
        let result = super::cell::with(|fs| f(fs)).expect("fs not mounted");
        ctx.out.write(result);
    }

    let mut ctx: Ctx<F, R> = Ctx {
        f: Some(f),
        out: MaybeUninit::uninit(),
    };
    let work = Work {
        run: trampoline::<F, R>,
        ctx: &mut ctx as *mut _ as *mut (),
        waiter: Task::current().expect("fs::submit requires a task context"),
    };
    let ptr = &work as *const Work as usize;
    queue()
        .send(ptr, Duration::infinite())
        .expect("fs worker queue send");
    CurrentTask::take_notification(true, Duration::infinite());
    // SAFETY: trampoline wrote `out` before notifying.
    unsafe { ctx.out.assume_init() }
}
