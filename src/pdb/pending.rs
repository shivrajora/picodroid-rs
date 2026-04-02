use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

extern crate alloc;
use alloc::vec::Vec;

/// When true, the JVM interpreter exits at the next opcode boundary.
pub static STOP_JVM: AtomicBool = AtomicBool::new(false);

/// Set by pdb_task before CMD_INSTALL flash operations.  When jvm_task sees
/// this after the JVM exits, it enters a RAM spin loop with interrupts disabled
/// so core 0 does not access flash during erase/program.
pub static FLASH_PARK_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Set by jvm_task (core 0) once it has entered the RAM spin loop.
/// pdb_task polls this before starting flash operations.
pub static CORE0_PARKED: AtomicBool = AtomicBool::new(false);

/// Set by pdb_task (core 1) when all flash operations are complete.
/// Core 0's spin loop exits when it sees this flag.
pub static CORE0_RELEASE: AtomicBool = AtomicBool::new(false);

/// Tracks the number of currently-running JVM child threads (spawned via Thread.start()).
/// jvm_task waits for this to reach zero before resetting the heap for a new app.
pub static ACTIVE_JVM_THREADS: AtomicU32 = AtomicU32::new(0);

// SAFETY: single-core; jvm_task and jvm-t tasks never run concurrently on RP2040/RP2350.
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
    // Wake core 0 from WFE if it is in the RP2350 spin-poll loop.
    cortex_m::asm::sev();
}

/// Register a child task. Called from the spawning side right after Task::start() returns.
pub fn register_child_task(task: freertos_rust::Task) {
    let n = ACTIVE_JVM_THREADS.load(Ordering::Relaxed);
    ACTIVE_JVM_THREADS.store(n + 1, Ordering::Relaxed);
    unsafe { (*CHILD_TASKS.0.get()).push(task) };
}

/// Deregister a child task by its raw handle. Called from within the child task
/// just before it exits, so jvm_task's wait loop can unblock.
pub fn deregister_child_task(own_handle: freertos_rust::FreeRtosTaskHandle) {
    let tasks = unsafe { &mut *CHILD_TASKS.0.get() };
    if let Some(pos) = tasks
        .iter()
        .position(|t| core::ptr::eq(t.raw_handle(), own_handle))
    {
        tasks.swap_remove(pos);
    }
    // Decrement and notify jvm_task when we are the last child to exit.
    // Single-core: no other task can race this load+store sequence.
    let n = ACTIVE_JVM_THREADS.load(Ordering::Relaxed);
    let next = n.saturating_sub(1);
    ACTIVE_JVM_THREADS.store(next, Ordering::Release);
    if next == 0 {
        notify_jvm();
    }
}

/// Abort delays on all registered child tasks. Called from jvm_task immediately
/// after run_jvm_with() returns so sleeping threads wake up and see STOP_JVM.
pub fn abort_all_child_delays() {
    let tasks = unsafe { &*CHILD_TASKS.0.get() };
    for task in tasks.iter() {
        task.abort_delay();
    }
}

/// Called by jvm_task at the start of each run cycle to clear the stop signal.
pub fn clear_stop() {
    STOP_JVM.store(false, Ordering::Relaxed);
}
