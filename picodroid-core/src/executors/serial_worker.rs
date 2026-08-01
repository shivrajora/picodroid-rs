// SPDX-License-Identifier: GPL-3.0-only
//! A worker task that runs submitted closures one at a time, to completion.
//!
//! Callers invoke [`SerialWorker::submit`] with a closure. The closure plus a
//! slot for its return value is placed on the *caller's* stack; a pointer to
//! that work item goes through a pointer-width queue and the caller blocks on
//! a task notification. The worker dequeues, runs the closure, and notifies
//! the caller.
//!
//! # What this buys
//!
//! Serialisation by construction. Because only the worker ever runs the
//! closures, two callers can never interleave inside whatever the closure
//! touches — each request runs to completion before the next is dequeued. A
//! mutex would give mutual exclusion but not this: it would still let the
//! critical section run on an arbitrary caller's task, at an arbitrary
//! priority, on an arbitrary core.
//!
//! That last part is why this exists rather than a lock. Its first caller is
//! the LittleFS worker (`crate::fs`), whose flash writes disable XIP and must
//! stay on one known core; pinning *the worker* achieves that for every
//! caller at once. The core affinity itself is platform policy and arrives
//! through [`crate::rtos::TaskKind`], not from here.
//!
//! # Lifetime contract
//!
//! `submit` blocks until the worker signals completion, so the `Ctx` on the
//! caller's stack outlives every access the worker makes to it. This is the
//! whole safety argument for the raw pointer in [`Work`], and it is why
//! `submit` must not gain a timeout: a caller that gave up early would leave
//! the worker writing into a dead frame.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;

use alloc::boxed::Box;

use crate::rtos::{self, RawQueue, RawTask, TaskKind, TaskSpec, Timeout};

/// How many requests may be in flight before a submitter blocks in `send`.
/// Requests are pointers to live caller frames, so this bounds concurrent
/// blocked callers, not memory the worker owns.
const QUEUE_CAPACITY: usize = 8;

struct Work {
    /// Trampoline that calls the type-erased caller closure.
    run: unsafe fn(*mut ()),
    /// Pointer to a caller-allocated `Ctx<F, R>` on the caller's stack.
    ctx: *mut (),
    /// The task that submitted this work; notified on completion.
    waiter: RawTask,
}

struct QueueCell(UnsafeCell<RawQueue>);
// SAFETY: installed exactly once in `spawn()` pre-scheduler, then read-only.
unsafe impl Sync for QueueCell {}

/// A serial worker: one task, one request queue.
///
/// Declared as a `static` by the owning module, so the queue handle has a
/// place to live that does not depend on an allocator being up.
pub struct SerialWorker {
    requests: QueueCell,
}

impl SerialWorker {
    pub const fn new() -> Self {
        Self {
            requests: QueueCell(UnsafeCell::new(0)),
        }
    }

    fn queue(&self) -> RawQueue {
        // SAFETY: written once in `spawn` before any caller can reach here.
        let q = unsafe { *self.requests.0.get() };
        debug_assert!(q != 0, "serial worker used before spawn()");
        q
    }

    /// Create the queue and the worker task.
    ///
    /// Must be called before the scheduler starts and before any [`submit`].
    /// `name` names the task for the platform's debugger and stack sizing;
    /// `kind` is what the platform maps to stack size, priority band and core
    /// affinity.
    ///
    /// [`submit`]: SerialWorker::submit
    pub fn spawn(&'static self, name: &'static str, kind: TaskKind, priority: u8) {
        let q = rtos::queue_create_ptr(QUEUE_CAPACITY);
        assert!(q != 0, "serial worker queue");
        // SAFETY: pre-scheduler, single-threaded.
        unsafe { *self.requests.0.get() = q };

        let spec = TaskSpec {
            name,
            kind,
            priority,
            stack_bytes: None, // platform's default for this kind
        };
        let spawned = rtos::spawn(
            &spec,
            Box::new(move || loop {
                let ptr = match rtos::queue_recv_ptr(self.queue(), Timeout::Forever) {
                    Some(p) => p,
                    None => continue,
                };
                // SAFETY: `ptr` points at a `Work` on the caller's stack; the
                // caller is blocked in `submit` until we notify, so the frame
                // is live for the duration of this call.
                let work = unsafe { &*(ptr as *const Work) };
                unsafe { (work.run)(work.ctx) };
                rtos::task_notify(work.waiter);
            }),
        );
        assert!(spawned, "serial worker task");
    }

    /// Submit `f` to the worker and block until it returns.
    ///
    /// The closure runs on the worker task's stack; anything it captures by
    /// reference stays valid, because this call does not return until the
    /// worker is done with it.
    pub fn submit<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        struct Ctx<F, R> {
            f: Option<F>,
            out: MaybeUninit<R>,
        }

        /// # Safety
        ///
        /// `ctx` must point at a live `Ctx<F, R>` whose `f` has not been taken.
        unsafe fn trampoline<F, R>(ctx: *mut ())
        where
            F: FnOnce() -> R,
        {
            let ctx = unsafe { &mut *(ctx as *mut Ctx<F, R>) };
            let f = ctx.f.take().expect("serial worker trampoline called twice");
            ctx.out.write(f());
        }

        let mut ctx: Ctx<F, R> = Ctx {
            f: Some(f),
            out: MaybeUninit::uninit(),
        };
        let work = Work {
            run: trampoline::<F, R>,
            ctx: &mut ctx as *mut _ as *mut (),
            waiter: rtos::task_current(),
        };
        let ptr = &work as *const Work as usize;
        assert!(
            rtos::queue_send_ptr(self.queue(), ptr, Timeout::Forever),
            "serial worker queue send"
        );
        rtos::task_wait_notification(Timeout::Forever);
        // SAFETY: the trampoline wrote `out` before the worker notified us.
        unsafe { ctx.out.assume_init() }
    }
}

impl Default for SerialWorker {
    fn default() -> Self {
        Self::new()
    }
}
