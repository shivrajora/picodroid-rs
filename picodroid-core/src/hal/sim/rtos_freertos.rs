// SPDX-License-Identifier: GPL-3.0-only
//! Host backing for [`crate::rtos::Rtos`] — the **real FreeRTOS kernel**,
//! POSIX port, running inside the simulator process.
//!
//! This backs every simulator run (`docs/designs/freertos-host-sim.md`).
//! Method for method it is the device implementation in
//! `platforms/rp/src/glue.rs`, minus core affinity — the POSIX port is
//! single-core — and plus the boot-budget model charges the device gets for
//! free by allocating from the same arena it is measured against.
//!
//! Its sibling [`super::rtos_std`] serves `cargo test` only, for a reason
//! documented in `hal/sim/mod.rs`; both expose the same free functions, so
//! callers name `hal::sim::rtos` and never the choice.
//!
//! # What the kernel buys over a host-thread model
//!
//! - `spawn` does not refuse [`TaskKind::JvmChild`]. A refusal would be
//!   needed if the object heap's single-core cooperative-scheduling guarantee
//!   had to come from host threads, which cannot provide it; the POSIX port
//!   provides exactly that guarantee — one task runs at a time, switched
//!   through the kernel — so `Thread.start` runs (parity-audit
//!   THR-01/THR-02, fix M7).
//! - the recursive mutex is the kernel's, not owner/depth bookkeeping over
//!   `std::sync::Mutex`.
//! - the tick is a FreeRTOS software timer, as on device, not a paced host
//!   thread.
//! - `delay_ms` is `vTaskDelay`, so sleeps quantise to the 1 kHz tick.
//!
//! # The one invariant callers must not break
//!
//! The port's "exactly one task runs at a time" only holds for code that
//! blocks *through the kernel*. A task that blocks on a std primitive (a
//! futex) can be released by a thread that is not the scheduler's current
//! task, and for a moment two pthreads run user code — precisely the overlap
//! the JVM heap must never see. Cross-task coordination therefore has to go
//! through this seam or through kernel objects. Host-service threads with no
//! device analog (the control-channel reader) stay outside the kernel and
//! communicate through atomics, which is fine: they never touch the heap.
//!
//! # Kernel memory is host-sized; the model is not
//!
//! `pvPortMalloc` is [`super::freertos_heap_shim`], which bypasses the
//! simulated arena. Device-sized bytes enter the arena through `charge_task`
//! instead — the platform's boot-budget model — so the arena keeps reporting
//! device figures while the kernel allocates host ones.

use alloc::boxed::Box;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};

use freertos_rust::{
    Duration, MutexInnerImpl, MutexRecursive, Queue, Semaphore, Task, TaskPriority, Timer,
};

use crate::rtos::{RawMutex, RawQueue, RawSem, RawTask, TaskSpec, Timeout};

extern "C" {
    /// Runs the scheduler on the calling thread. Unlike a device port this
    /// one *returns*, when [`end_scheduler`] is called — which is why this is
    /// declared here rather than taken from `freertos_rust`, whose
    /// `FreeRtosUtils::start_scheduler` is typed `-> !`.
    fn vTaskStartScheduler();
    fn vTaskEndScheduler();
}

/// Hand the calling thread to the scheduler. Returns once a task has called
/// [`end_scheduler`]; every task must already have been created.
pub fn start_scheduler() {
    // SAFETY: FFI into the kernel. The caller is the process's main thread
    // (see `platforms/rp/src/main.rs`), which is what the POSIX port assumes.
    unsafe { vTaskStartScheduler() }
}

/// Stop the scheduler and release the thread blocked in [`start_scheduler`].
///
/// One app per process: the POSIX port only resets its `pthread_once` state
/// on macOS (`port.c:309-314`), so an in-process restart is out of scope.
pub fn end_scheduler() {
    // SAFETY: FFI into the kernel, from a task context.
    unsafe { vTaskEndScheduler() }
}

fn to_duration(t: Timeout) -> Duration {
    match t {
        Timeout::None => Duration::zero(),
        Timeout::Ms(ms) => Duration::ms(ms),
        Timeout::Forever => Duration::infinite(),
    }
}

struct TimerCell(core::cell::UnsafeCell<Option<Timer>>);
// SAFETY: mutated only by the UI task before the timer is in active use;
// afterwards `Timer`'s operations go through the FreeRTOS timer command queue
// and are themselves task-safe. Identical to the device's reasoning.
unsafe impl Sync for TimerCell {}

static TICK_TIMER: TimerCell = TimerCell(core::cell::UnsafeCell::new(None));

/// Java threads that have been started and have not yet returned.
///
/// The device tracks the same thing in its debug bridge (`pdb::pending::
/// ACTIVE_JVM_THREADS`) so the JVM task can wait for its children before
/// letting an install reboot the app. The simulator has no debug bridge but
/// needs the same number for the same reason: without it, an app whose
/// `onCreate` starts threads and returns would end the scheduler out from
/// under children that had not run a single instruction — which is how
/// `threaddemo` looked before this existed, and indistinguishable from the
/// no-op the test backing deliberately performs.
static LIVE_JVM_CHILDREN: AtomicUsize = AtomicUsize::new(0);

/// Java threads still running. See [`LIVE_JVM_CHILDREN`].
pub fn live_jvm_children() -> usize {
    LIVE_JVM_CHILDREN.load(Ordering::Acquire)
}

std::thread_local! {
    /// True on every pthread that entered through [`spawn`]'s trampoline —
    /// i.e. is a FreeRTOS task. Every task in the simulator is created
    /// through that one trampoline (`sim_boot` and the executors all go
    /// through the `crate::rtos` seam), so this is a complete census; host
    /// service threads with no device analog (the control-channel reader)
    /// stay false.
    static IS_KERNEL_TASK: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Whether the calling thread is a FreeRTOS task, and may therefore use
/// task-only kernel APIs (`vTaskDelay` and friends). Kernel task APIs called
/// from a non-task pthread act on whatever task the kernel believes is
/// current, so callers that can run on either kind of thread must check.
pub fn current_thread_is_task() -> bool {
    IS_KERNEL_TASK.with(|f| f.get())
}

/// Spawn `spec` as a real FreeRTOS task.
///
/// `charge_task` bills the platform's boot-budget model and returns the
/// **device** stack size in bytes for this spec — the number the device would
/// have allocated. That single return value does double duty: it is the arena
/// charge, and it is what sizes the kernel task here, so the two can never
/// disagree. Sizing policy stays with the platform, exactly as
/// [`crate::rtos`] requires.
///
/// `release_task` undoes the charge when the task's body returns. The device
/// gets this for free — `vTaskDelete(NULL)` returns the stack and TCB to the
/// heap they came from — and without it every finished Java thread would leak
/// its 16 KB from the model (`boot_budget.rs`'s deliberate leak, now paired).
/// The task itself does not end here; see [`park_finished_task`].
pub fn spawn(
    spec: &TaskSpec,
    body: Box<dyn FnOnce() + Send>,
    charge_task: fn(&TaskSpec) -> u32,
    release_task: fn(&TaskSpec),
) -> bool {
    let stack_bytes = charge_task(spec);
    // FreeRTOS counts stacks in words, and this is the only place the seam's
    // bytes become them. On this port the "stack" allocation holds nothing but
    // the port's `Thread_t` — real stacks come from pthread's own default — so
    // the count exists to keep `uxTaskGetStackHighWaterMark` meaningful, not to
    // bound recursion. The floor is the port's requirement (`Thread_t` must
    // fit, `port.c:230`); it never applies to a real task kind, and it never
    // touches what was charged.
    let words = (stack_bytes / 4).clamp(128, u16::MAX as u32) as u16;
    let spec = *spec;
    let is_child = spec.kind == crate::rtos::TaskKind::JvmChild;

    if is_child {
        // Before the spawn, not inside the body: the starting task must be
        // able to see the count go up the moment `Thread.start` returns, or a
        // `start(); return;` in `onCreate` races the child's first
        // instruction.
        LIVE_JVM_CHILDREN.fetch_add(1, Ordering::AcqRel);
    }

    let spawned = Task::new()
        .name(spec.name)
        .stack_size(words)
        .priority(TaskPriority(spec.priority))
        .start(move |_| {
            IS_KERNEL_TASK.with(|f| f.set(true));
            // Unwinding out of the port's `extern "C"` task trampoline is UB,
            // and abort-on-panic is what the device does under panic-probe.
            if std::panic::catch_unwind(AssertUnwindSafe(body)).is_err() {
                eprintln!("[sim] task '{}' panicked — aborting", spec.name);
                std::process::abort();
            }
            release_task(&spec);
            if is_child {
                LIVE_JVM_CHILDREN.fetch_sub(1, Ordering::AcqRel);
            }
            park_finished_task();
        });

    if spawned.is_err() {
        // The charge was for a task that does not exist.
        release_task(&spec);
        if is_child {
            LIVE_JVM_CHILDREN.fetch_sub(1, Ordering::AcqRel);
        }
        return false;
    }
    true
}

/// Suspend a task whose body has returned, forever, instead of letting it end.
///
/// A task that finishes normally reaches `vTaskDelete(NULL)` in
/// `freertos-rust`'s spawn trampoline, and the POSIX port implements the end of
/// a task with `pthread_exit()` (`port.c:592-595`). `pthread_exit` performs a
/// *forced unwind*, which has to pass back through that trampoline — a Rust
/// `extern "C"` function, and therefore `nounwind`. The unwinder finds a frame
/// it is not allowed to cross and aborts the process. Every Java thread whose
/// `run()` returned would kill the simulator, and only apps whose threads loop
/// forever (`threaddemo` as shipped) would appear to work.
///
/// Deleting the task from *another* task is no better: the port's
/// `vPortCancelThread` uses `pthread_cancel`, which unwinds the same frames.
///
/// So the task parks. The model charge for its stack and TCB has already been
/// released just above — the arena, which is what parity measures, is exactly
/// as correct as if the task had ended. What leaks is host-side and uncounted:
/// one suspended pthread and its kernel TCB per finished Java thread. An app
/// that starts a few hundred threads over its life is fine; one that churns
/// tens of thousands would run the host out of threads, which is a simulator
/// limit worth knowing rather than a parity divergence.
///
/// The real fix belongs in the fork: `thread_start` declared `extern
/// "C-unwind"` would let the forced unwind through legally (no Rust
/// destructors are live in that frame by then). That needs a
/// `freertos-rust-pd` point release; see docs/designs/freertos-host-sim.md.
fn park_finished_task() -> ! {
    loop {
        freertos_rust::CurrentTask::suspend();
    }
}

pub fn queue_create(depth: usize) -> RawQueue {
    match Queue::<u32>::new(depth) {
        Ok(q) => Box::into_raw(Box::new(q)) as RawQueue,
        Err(_) => 0,
    }
}

pub fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool {
    if q == 0 {
        return false;
    }
    // SAFETY: `q` came from `queue_create`, which leaks a `Box<Queue<u32>>`
    // for the process — the same handle contract the device arm uses.
    let queue = unsafe { &*(q as *const Queue<u32>) };
    queue.send(word, to_duration(t)).is_ok()
}

pub fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32> {
    if q == 0 {
        return None;
    }
    // SAFETY: see `queue_send`.
    let queue = unsafe { &*(q as *const Queue<u32>) };
    queue.receive(to_duration(t)).ok()
}

pub fn task_current() -> RawTask {
    // Pre-scheduler, or one of the host-service threads the kernel does not
    // own (the control-channel reader): no task context, spelled 0.
    Task::current()
        .map(|t| t.raw_handle() as RawTask)
        .unwrap_or(0)
}

pub fn scheduler_running() -> bool {
    freertos_rust::FreeRtosUtils::scheduler_state()
        == freertos_rust::FreeRtosSchedulerState::Running
}

pub fn task_notify(t: RawTask) {
    if t == 0 {
        return;
    }
    // SAFETY: `t` is a live handle from `task_current` — the seam's contract.
    // `Task` wraps a handle and has no `Drop`, so this neither owns nor
    // deletes the task. Identical to the device arm.
    let task = unsafe { Task::from_raw_handle(t as *const core::ffi::c_void) };
    task.notify(freertos_rust::TaskNotification::Increment);
}

pub fn task_wait_notification(t: Timeout) -> bool {
    freertos_rust::CurrentTask::take_notification(true, to_duration(t)) != 0
}

pub fn queue_create_ptr(depth: usize) -> RawQueue {
    match Queue::<usize>::new(depth) {
        Ok(q) => Box::into_raw(Box::new(q)) as RawQueue,
        Err(_) => 0,
    }
}

pub fn queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool {
    if q == 0 {
        return false;
    }
    // SAFETY: see `queue_send`.
    let queue = unsafe { &*(q as *const Queue<usize>) };
    queue.send(val, to_duration(t)).is_ok()
}

pub fn queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize> {
    if q == 0 {
        return None;
    }
    // SAFETY: see `queue_send`.
    let queue = unsafe { &*(q as *const Queue<usize>) };
    queue.receive(to_duration(t)).ok()
}

pub fn mutex_recursive_create() -> Option<RawMutex> {
    MutexRecursive::create()
        .ok()
        .map(|m| Box::into_raw(Box::new(m)) as RawMutex)
}

pub fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool {
    if m == 0 {
        return false;
    }
    // `take`/`give` rather than a scoped guard: `monitorenter` and
    // `monitorexit` are separate bytecodes, so no Rust scope spans the
    // critical region.
    //
    // SAFETY: see `queue_send`.
    let mutex = unsafe { &*(m as *const MutexRecursive) };
    mutex.take(to_duration(t)).is_ok()
}

pub fn mutex_recursive_unlock(m: RawMutex) {
    if m == 0 {
        return;
    }
    // SAFETY: see `queue_send`.
    let mutex = unsafe { &*(m as *const MutexRecursive) };
    mutex.give();
}

pub fn mutex_recursive_delete(m: RawMutex) {
    if m == 0 {
        return;
    }
    // SAFETY: `m` came from `mutex_recursive_create`, which leaked a
    // `Box<MutexRecursive>`; re-boxing drops it, and its `Drop` deletes the
    // kernel semaphore.
    drop(unsafe { Box::from_raw(m as *mut MutexRecursive) });
}

pub fn sem_binary_create() -> RawSem {
    match Semaphore::new_binary() {
        Ok(s) => Box::into_raw(Box::new(s)) as RawSem,
        Err(_) => 0,
    }
}

pub fn sem_give(s: RawSem) {
    if s == 0 {
        return;
    }
    // SAFETY: see `queue_send`.
    let sem = unsafe { &*(s as *const Semaphore) };
    sem.give();
}

pub fn sem_take(s: RawSem, t: Timeout) -> bool {
    if s == 0 {
        return false;
    }
    // SAFETY: see `queue_send`.
    let sem = unsafe { &*(s as *const Semaphore) };
    sem.take(to_duration(t)).is_ok()
}

pub fn tick_timer_start(period_ms: u32, cb: fn()) {
    // SAFETY: callers serialise on the UI task (see run_activity), which is
    // the device arm's justification too.
    unsafe {
        if let Some(t) = (*TICK_TIMER.0.get()).as_ref() {
            let _ = t.start(Duration::ms(0));
            return;
        }
        let timer = Timer::new(Duration::ms(period_ms))
            .set_name("lvgl-tick")
            .set_auto_reload(true)
            .create(move |_| cb())
            .expect("lvgl-tick timer alloc");
        timer.start(Duration::ms(0)).expect("lvgl-tick start");
        *TICK_TIMER.0.get() = Some(timer);
    }
}

pub fn tick_timer_pause() {
    // SAFETY: see `tick_timer_start`.
    unsafe {
        if let Some(t) = (*TICK_TIMER.0.get()).as_ref() {
            let _ = t.stop(Duration::ms(0));
        }
    }
}

pub fn tick_timer_resume() {
    // SAFETY: see `tick_timer_start`.
    unsafe {
        if let Some(t) = (*TICK_TIMER.0.get()).as_ref() {
            let _ = t.start(Duration::ms(0));
        }
    }
}

/// Stop the timer but keep the allocation.
///
/// A host-thread backing has to *join* a thread here or an app reload leaks
/// one per run; this one has no thread, and dropping a `Timer` blocks the
/// caller for up to a second on the timer command queue. So, like the device,
/// it stops and holds — the tick is a process-wide singleton either way.
pub fn tick_timer_stop() {
    tick_timer_pause();
}

pub fn delay_ms(ms: u32) {
    freertos_rust::CurrentTask::delay(Duration::ms(ms));
}
