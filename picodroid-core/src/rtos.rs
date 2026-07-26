// SPDX-License-Identifier: GPL-3.0-only
//! RTOS abstraction — tasks, queues, mutexes, semaphores, and the UI tick.
//!
//! Six framework files used `freertos_rust` directly, against a dependency
//! declared only under `cfg(target_arch = "arm")` of the binary crate. That
//! is the second of the three things that kept the framework trapped there
//! (`docs/designs/shared-core-extraction.md` §1.2); this trait is the
//! replacement. It covers exactly what those files use — nothing
//! speculative.
//!
//! Each of those files already carried an inline device/sim split (`mod
//! backing`, or `mod device` beside `mod sim`); this generalises that shape
//! into something a second family can implement.
//!
//! # Stack sizes are bytes
//!
//! FreeRTOS counts stacks in *words*, ESP-IDF's port counts them in *bytes*
//! (`ARCHITECTURE.md:99-103`). Baking either unit into the seam would encode
//! one family's convention into shared code — precisely the class of
//! assumption this extraction exists to remove — so [`TaskSpec::stack_bytes`]
//! is bytes and each platform converts.
//!
//! # Policy stays with the platform
//!
//! Shared code says *what kind* of task it wants, not how big or where it
//! runs: core affinity, stack sizing from the boot budget, and any
//! debugger-visible task registration live in the platform's [`Rtos`] impl.

/// What a task is for. The platform maps this to stack size, core affinity,
/// and any bookkeeping it needs (e.g. registering JVM child tasks with a
/// debug bridge so a stop request can reach them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    /// A Java `Thread` started by app code.
    JvmChild,
    /// A worker in the background executor pool.
    BgWorker,
    /// The sensor sampling task.
    Sensor,
}

/// How long a blocking operation may wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timeout {
    /// Return immediately if the operation would block.
    None,
    Ms(u32),
    /// Block until the operation succeeds.
    Forever,
}

/// A task to spawn.
#[derive(Clone, Copy, Debug)]
pub struct TaskSpec {
    pub name: &'static str,
    pub kind: TaskKind,
    /// Android-tier priority, already mapped through
    /// [`crate::task_priority`].
    pub priority: u8,
    /// Stack size in **bytes**, or `None` for the platform's default for
    /// this [`TaskKind`].
    pub stack_bytes: Option<u32>,
}

/// Opaque platform handles. The platform decides what these mean (a FreeRTOS
/// handle pointer, an index into a table, …); shared code only passes them
/// back.
pub type RawQueue = usize;
pub type RawMutex = usize;
pub type RawSem = usize;

/// The RTOS services shared code needs.
///
/// # Safety
///
/// Implementations must provide genuine mutual exclusion and cross-task
/// wakeups: the framework relies on `queue_recv` blocking the caller (it is
/// the UI thread's idle point) and on `mutex_recursive_*` being reentrant
/// from the same task, since Java `synchronized` re-enters.
pub unsafe trait Rtos {
    /// Spawn a task. `entry` is called on the new task with `arg`.
    /// Returns false if the platform could not create it.
    fn spawn(spec: &TaskSpec, entry: fn(u32), arg: u32) -> bool;

    fn queue_create(depth: usize) -> RawQueue;
    fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool;
    fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32>;

    /// Create a *recursive* mutex — Java monitors re-enter.
    fn mutex_recursive_create() -> Option<RawMutex>;
    fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool;
    fn mutex_recursive_unlock(m: RawMutex);

    fn sem_binary_create() -> RawSem;
    fn sem_give(s: RawSem);
    fn sem_take(s: RawSem, t: Timeout) -> bool;

    /// Start the periodic UI tick. Singleton: called once at boot.
    fn tick_timer_start(period_ms: u32, cb: fn());
    fn tick_timer_pause();
    fn tick_timer_resume();

    /// Block the calling task.
    fn delay_ms(ms: u32);
}

extern "Rust" {
    fn __pd_rtos_spawn(spec: &TaskSpec, entry: fn(u32), arg: u32) -> bool;
    fn __pd_rtos_queue_create(depth: usize) -> RawQueue;
    fn __pd_rtos_queue_send(q: RawQueue, word: u32, t: Timeout) -> bool;
    fn __pd_rtos_queue_recv(q: RawQueue, t: Timeout) -> Option<u32>;
    fn __pd_rtos_mutex_recursive_create() -> Option<RawMutex>;
    fn __pd_rtos_mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool;
    fn __pd_rtos_mutex_recursive_unlock(m: RawMutex);
    fn __pd_rtos_sem_binary_create() -> RawSem;
    fn __pd_rtos_sem_give(s: RawSem);
    fn __pd_rtos_sem_take(s: RawSem, t: Timeout) -> bool;
    fn __pd_rtos_tick_timer_start(period_ms: u32, cb: fn());
    fn __pd_rtos_tick_timer_pause();
    fn __pd_rtos_tick_timer_resume();
    fn __pd_rtos_delay_ms(ms: u32);
}

pub fn spawn(spec: &TaskSpec, entry: fn(u32), arg: u32) -> bool {
    unsafe { __pd_rtos_spawn(spec, entry, arg) }
}
pub fn queue_create(depth: usize) -> RawQueue {
    unsafe { __pd_rtos_queue_create(depth) }
}
pub fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool {
    unsafe { __pd_rtos_queue_send(q, word, t) }
}
pub fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32> {
    unsafe { __pd_rtos_queue_recv(q, t) }
}
pub fn mutex_recursive_create() -> Option<RawMutex> {
    unsafe { __pd_rtos_mutex_recursive_create() }
}
pub fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool {
    unsafe { __pd_rtos_mutex_recursive_lock(m, t) }
}
pub fn mutex_recursive_unlock(m: RawMutex) {
    unsafe { __pd_rtos_mutex_recursive_unlock(m) }
}
pub fn sem_binary_create() -> RawSem {
    unsafe { __pd_rtos_sem_binary_create() }
}
pub fn sem_give(s: RawSem) {
    unsafe { __pd_rtos_sem_give(s) }
}
pub fn sem_take(s: RawSem, t: Timeout) -> bool {
    unsafe { __pd_rtos_sem_take(s, t) }
}
pub fn tick_timer_start(period_ms: u32, cb: fn()) {
    unsafe { __pd_rtos_tick_timer_start(period_ms, cb) }
}
pub fn tick_timer_pause() {
    unsafe { __pd_rtos_tick_timer_pause() }
}
pub fn tick_timer_resume() {
    unsafe { __pd_rtos_tick_timer_resume() }
}
pub fn delay_ms(ms: u32) {
    unsafe { __pd_rtos_delay_ms(ms) }
}

/// Register the platform's [`Rtos`] implementation.
///
/// See [`crate::hal`]'s macros for the symbol-name contract these share.
#[macro_export]
macro_rules! set_rtos {
    ($t:ty) => {
        const _: () = {
            use $crate::rtos::{RawMutex, RawQueue, RawSem, TaskSpec, Timeout};

            #[no_mangle]
            extern "Rust" fn __pd_rtos_spawn(spec: &TaskSpec, entry: fn(u32), arg: u32) -> bool {
                <$t as $crate::rtos::Rtos>::spawn(spec, entry, arg)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_queue_create(depth: usize) -> RawQueue {
                <$t as $crate::rtos::Rtos>::queue_create(depth)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_queue_send(q: RawQueue, word: u32, t: Timeout) -> bool {
                <$t as $crate::rtos::Rtos>::queue_send(q, word, t)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_queue_recv(q: RawQueue, t: Timeout) -> Option<u32> {
                <$t as $crate::rtos::Rtos>::queue_recv(q, t)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_mutex_recursive_create() -> Option<RawMutex> {
                <$t as $crate::rtos::Rtos>::mutex_recursive_create()
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool {
                <$t as $crate::rtos::Rtos>::mutex_recursive_lock(m, t)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_mutex_recursive_unlock(m: RawMutex) {
                <$t as $crate::rtos::Rtos>::mutex_recursive_unlock(m)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_sem_binary_create() -> RawSem {
                <$t as $crate::rtos::Rtos>::sem_binary_create()
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_sem_give(s: RawSem) {
                <$t as $crate::rtos::Rtos>::sem_give(s)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_sem_take(s: RawSem, t: Timeout) -> bool {
                <$t as $crate::rtos::Rtos>::sem_take(s, t)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_tick_timer_start(period_ms: u32, cb: fn()) {
                <$t as $crate::rtos::Rtos>::tick_timer_start(period_ms, cb)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_tick_timer_pause() {
                <$t as $crate::rtos::Rtos>::tick_timer_pause()
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_tick_timer_resume() {
                <$t as $crate::rtos::Rtos>::tick_timer_resume()
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_delay_ms(ms: u32) {
                <$t as $crate::rtos::Rtos>::delay_ms(ms)
            }
        };
    };
}
