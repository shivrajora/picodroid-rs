// SPDX-License-Identifier: GPL-3.0-only
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

extern crate alloc;
use alloc::vec::Vec;

/// When true, the JVM interpreter exits at the next opcode boundary.
pub static STOP_JVM: AtomicBool = AtomicBool::new(false);

/// Set STOP_JVM.  PDB and JVM run on the same core so a plain
/// Relaxed store is immediately visible — no cross-core barriers needed.
#[cfg(not(feature = "sim"))]
pub fn set_stop_jvm() {
    STOP_JVM.store(true, Ordering::Relaxed);
}

/// Check if the JVM should stop.
#[cfg(not(feature = "sim"))]
pub fn is_stop_jvm() -> bool {
    STOP_JVM.load(Ordering::Relaxed)
}

/// Set by pdb_task before CMD_INSTALL flash operations.  When jvm_task sees
/// this after the JVM exits, it signals `CORE0_PARKED` and blocks on a
/// FreeRTOS notification so it does not touch flash (XIP) while pdb_task —
/// on the same core since the core-0 move — erases/programs.
pub static FLASH_PARK_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Set by jvm_task once it has signalled and blocked.
/// pdb_task polls this before starting flash operations.
pub static CORE0_PARKED: AtomicBool = AtomicBool::new(false);

/// Tracks the number of JVM child threads (spawned via Thread.start()) that
/// have been counted by [`note_child_spawning`] and not yet deregistered.
/// jvm_task waits for this to reach zero before resetting the heap for a new
/// app. Plain load/store (thumbv6m has no atomic RMW) — every read-modify-
/// write below runs inside an `AtomicSection`, which is what makes it atomic
/// against the other JVM tasks that also touch it.
pub static ACTIVE_JVM_THREADS: AtomicU32 = AtomicU32::new(0);

// SAFETY: every access is inside an `AtomicSection` (scheduler suspended),
// so no two tasks can be in the Vec at once. The former "single-core, never
// concurrent" argument did not survive unequal priorities: a child created
// above its parent's priority preempts it inside `Vec::push`.
struct ChildTasksCell(UnsafeCell<Vec<freertos_rust::Task>>);
unsafe impl Sync for ChildTasksCell {}
// Vec::new() is const and does not allocate — safe in a static initializer.
static CHILD_TASKS: ChildTasksCell = ChildTasksCell(UnsafeCell::new(Vec::new()));

// SAFETY: written once by jvm_task at startup before pdb_task or any child task can call
// notify_jvm(); read-only after that. Single-core, no concurrent writes.
struct TaskCell(UnsafeCell<Option<freertos_rust::Task>>);
unsafe impl Sync for TaskCell {}
static JVM_TASK: TaskCell = TaskCell(UnsafeCell::new(None));

/// Store the jvm_task handle so pdb_task and child tasks can wake it.
/// Must be called once at the start of the jvm_task closure.
pub fn set_jvm_task(task: freertos_rust::Task) {
    unsafe { *JVM_TASK.0.get() = Some(task) };
}

/// Increment jvm_task's notification value, waking it if it is blocked on
/// `CurrentTask::take_notification`.
pub(super) fn notify_jvm() {
    if let Some(t) = unsafe { (*JVM_TASK.0.get()).as_ref() } {
        t.notify(freertos_rust::TaskNotification::Increment);
    }
    // The activity loop blocks on the main queue, not on a task notification.
    // Post a Wake sentinel so `recv_blocking` returns and the loop's next
    // iteration sees `handler.interrupted()` (i.e. STOP_JVM). Wake bypasses
    // tick coalescing, so it's safe to call from any task without violating
    // the tick-source-owns-`TICK_IN_QUEUE` invariant.
    picodroid_core::executors::main_queue::enqueue_wake();
    // Wake core 0 from WFE if it is in the RP2350 poll loop.
    cortex_m::asm::sev();
}

/// Count a child that is about to be created. Called by the spawning side
/// *before* `Task::start`, so the count is already up when the child runs
/// its first instruction — a child created at a higher priority than its
/// parent does exactly that, before `start` returns.
pub fn note_child_spawning() {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let n = ACTIVE_JVM_THREADS.load(Ordering::Relaxed);
    ACTIVE_JVM_THREADS.store(n + 1, Ordering::Relaxed);
}

/// Undo [`note_child_spawning`] when `Task::start` failed and no child will
/// ever deregister.
pub fn abort_child_spawn() {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let n = ACTIVE_JVM_THREADS.load(Ordering::Relaxed);
    ACTIVE_JVM_THREADS.store(n.saturating_sub(1), Ordering::Relaxed);
}

/// Register a child task's handle. Called by the child itself, as the first
/// thing it does, so a handle in this list always names a live task (the
/// spawning side never holds one — see `glue.rs`). Does not touch the
/// count; [`note_child_spawning`] already did.
pub fn register_child_task(task: freertos_rust::Task) {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    unsafe { (*CHILD_TASKS.0.get()).push(task) };
}

/// Deregister a child task by its raw handle. Called from within the child task
/// just before it exits, so jvm_task's wait loop can unblock.
pub fn deregister_child_task(own_handle: freertos_rust::FreeRtosTaskHandle) {
    let next = {
        let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
        let tasks = unsafe { &mut *CHILD_TASKS.0.get() };
        if let Some(pos) = tasks
            .iter()
            .position(|t| core::ptr::eq(t.raw_handle(), own_handle))
        {
            tasks.swap_remove(pos);
        }
        let n = ACTIVE_JVM_THREADS.load(Ordering::Relaxed);
        let next = n.saturating_sub(1);
        ACTIVE_JVM_THREADS.store(next, Ordering::Release);
        next
    };
    // Notify jvm_task when we are the last child to exit — outside the
    // section, since notifying may yield.
    if next == 0 {
        notify_jvm();
    }
}

/// Abort delays on all registered child tasks. Called from jvm_task immediately
/// after run_jvm_with() returns so sleeping threads wake up and see STOP_JVM.
pub fn abort_all_child_delays() {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let tasks = unsafe { &*CHILD_TASKS.0.get() };
    for task in tasks.iter() {
        // Non-blocking (`xTaskAbortDelay` only readies the task), so it is
        // legal inside the section; the readied child cannot run until the
        // scheduler resumes.
        task.abort_delay();
    }
}

/// Called by jvm_task at the start of each run cycle to clear the stop signal.
pub fn clear_stop() {
    STOP_JVM.store(false, Ordering::Relaxed);
}
