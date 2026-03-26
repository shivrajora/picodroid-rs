use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

extern crate alloc;
use alloc::vec::Vec;

pub const MAX_PAPK_SIZE: usize = 65536;

// SAFETY: single-core RP2040/RP2350; only pdb_task writes, jvm_task reads after
// STOP_JVM is acknowledged. No concurrent writes occur.
struct PapkBufCell(UnsafeCell<[u8; MAX_PAPK_SIZE]>);
unsafe impl Sync for PapkBufCell {}
static PAPK_BUF: PapkBufCell = PapkBufCell(UnsafeCell::new([0u8; MAX_PAPK_SIZE]));

pub(super) static PAPK_LEN: AtomicUsize = AtomicUsize::new(0);
pub static HAS_PENDING: AtomicBool = AtomicBool::new(false);

/// When true, the JVM interpreter exits at the next opcode boundary.
pub static STOP_JVM: AtomicBool = AtomicBool::new(false);

/// Tracks the number of currently-running JVM child threads (spawned via Thread.start()).
/// jvm_task waits for this to reach zero before resetting the heap for a new app.
pub static ACTIVE_JVM_THREADS: AtomicU32 = AtomicU32::new(0);

// SAFETY: single-core; jvm_task and jvm-t tasks never run concurrently on RP2040/RP2350.
struct ChildTasksCell(UnsafeCell<Vec<freertos_rust::Task>>);
unsafe impl Sync for ChildTasksCell {}
// Vec::new() is const and does not allocate — safe in a static initializer.
static CHILD_TASKS: ChildTasksCell = ChildTasksCell(UnsafeCell::new(Vec::new()));

/// Register a child task. Called from the spawning side right after Task::start() returns.
pub fn register_child_task(task: freertos_rust::Task) {
    let n = ACTIVE_JVM_THREADS.load(Ordering::Relaxed);
    ACTIVE_JVM_THREADS.store(n + 1, Ordering::Relaxed);
    unsafe { (*CHILD_TASKS.0.get()).push(task) };
}

/// Deregister a child task by its raw handle. Called from within the child task
/// just before it enters its idle sleep, so jvm_task's wait loop can unblock.
pub fn deregister_child_task(own_handle: freertos_rust::FreeRtosTaskHandle) {
    let tasks = unsafe { &mut *CHILD_TASKS.0.get() };
    if let Some(pos) = tasks
        .iter()
        .position(|t| core::ptr::eq(t.raw_handle(), own_handle))
    {
        tasks.swap_remove(pos);
    }
    let n = ACTIVE_JVM_THREADS.load(Ordering::Relaxed);
    ACTIVE_JVM_THREADS.store(n.saturating_sub(1), Ordering::Release);
}

/// Abort delays on all registered child tasks. Called from jvm_task immediately
/// after run_jvm_with() returns so sleeping threads wake up and see STOP_JVM.
pub fn abort_all_child_delays() {
    let tasks = unsafe { &*CHILD_TASKS.0.get() };
    for task in tasks.iter() {
        task.abort_delay();
    }
}

/// Exposes the raw buffer pointer for pdb_task to stream bytes directly into.
/// # Safety
/// Caller must ensure no concurrent access with jvm_task reads.
pub(super) unsafe fn buf_mut() -> *mut u8 {
    (*PAPK_BUF.0.get()).as_mut_ptr()
}

/// Returns a `'static` slice into the PAPK buffer for the installed PAPK.
/// Called only from jvm_task (single consumer); load+store is sufficient on
/// single-core ARM where aligned accesses are naturally atomic.
pub fn take() -> Option<&'static [u8]> {
    if HAS_PENDING.load(Ordering::Relaxed) {
        HAS_PENDING.store(false, Ordering::Relaxed);
        let len = PAPK_LEN.load(Ordering::Relaxed);
        Some(unsafe { core::slice::from_raw_parts((*PAPK_BUF.0.get()).as_ptr(), len) })
    } else {
        None
    }
}

/// Called by jvm_task at the start of each run cycle to clear the stop signal.
pub fn clear_stop() {
    STOP_JVM.store(false, Ordering::Relaxed);
}
