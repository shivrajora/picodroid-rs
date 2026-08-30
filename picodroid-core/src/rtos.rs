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

use alloc::boxed::Box;

/// What a task is for. The platform maps this to stack size, core affinity,
/// and any bookkeeping it needs (e.g. registering JVM child tasks with a
/// debug bridge so a stop request can reach them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    /// The task the framework itself runs inside — the interpreter, the UI
    /// loop, the activity lifecycle.
    ///
    /// Only the simulator creates this through the seam ([`crate::sim_boot`]).
    /// A device's boot code creates its JVM task directly, because it also
    /// pins it to a core and wraps it in a supervisor loop that no seam should
    /// have to describe; the kind exists so both take their stack size from
    /// the same place.
    Jvm,
    /// A Java `Thread` started by app code.
    JvmChild,
    /// A worker in the background executor pool.
    BgWorker,
    /// The sensor sampling task.
    Sensor,
    /// The filesystem's serial worker
    /// ([`crate::executors::serial_worker`]).
    ///
    /// A platform whose filesystem writes need a particular core — because
    /// they disable execute-in-place on the one they run from, as the RP
    /// family's do — pins this task, and every caller inherits that for free.
    FsWorker,
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

/// A task, as something another task can wake. `0` means "no task context" —
/// [`Rtos::task_current`] returns it when called before the scheduler runs or
/// from a host thread the RTOS does not own, and [`Rtos::task_notify`] ignores
/// it. Callers therefore never have to special-case pre-scheduler code.
pub type RawTask = usize;

/// The RTOS services shared code needs.
///
/// # Safety
///
/// Implementations must provide genuine mutual exclusion and cross-task
/// wakeups: the framework relies on `queue_recv` blocking the caller (it is
/// the UI thread's idle point) and on `mutex_recursive_*` being reentrant
/// from the same task, since Java `synchronized` re-enters.
pub unsafe trait Rtos {
    /// Spawn a task running `body`. Returns false if the platform declined
    /// or could not create it.
    ///
    /// A boxed closure rather than a bare `fn` pointer: the work a task
    /// carries (a Java `Runnable`'s object ref and its class name, a worker
    /// index) does not fit in a machine word, and both backings box their
    /// entry point internally anyway, so this costs nothing extra.
    ///
    /// Declining is a legitimate answer. The `cargo test` backing refuses
    /// [`TaskKind::JvmChild`] because the object heap's safety rests on
    /// "JVM tasks switch only at kernel yield points", which host threads do
    /// not provide, and no scheduler is running there. The simulator proper
    /// runs the real FreeRTOS kernel and accepts it.
    ///
    /// A platform that tracks live children for a debug bridge must count
    /// the child *before* creating it and let the child register its own
    /// handle: a child created above its parent's priority can run to
    /// completion before this function returns.
    fn spawn(spec: &TaskSpec, body: Box<dyn FnOnce() + Send>) -> bool;

    fn queue_create(depth: usize) -> RawQueue;
    fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool;
    fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32>;

    /// Handle of the calling task, or 0 if there is no task context.
    fn task_current() -> RawTask;

    /// Whether the scheduler has started and is running tasks.
    ///
    /// Shared code needs this to tell "I am early in boot, nothing else can
    /// run, do the work inline" from "there are tasks, go through the worker"
    /// — [`crate::fs::with_fs`] is the caller. [`Rtos::task_current`] cannot
    /// answer it: FreeRTOS assigns a current task at the first *task
    /// creation*, not at scheduler start, so it reports a live handle during
    /// boot and a caller that trusted it would block for a notification no
    /// running task could send.
    fn scheduler_running() -> bool;

    /// Increment `t`'s notification count, waking it if it is blocked in
    /// [`Rtos::task_wait_notification`]. A no-op when `t` is 0.
    ///
    /// Increment rather than set, because the notification is a wakeup and
    /// not a value: two notifiers racing must not collapse into one wake.
    fn task_notify(t: RawTask);

    /// Block until the calling task's notification count is non-zero, then
    /// clear it and return true. False means `t` elapsed first.
    ///
    /// Clearing rather than decrementing matches how the framework uses it:
    /// every waiter re-checks the condition it actually cares about (a
    /// counter, a flag) after waking, so a notification is "look again", not
    /// a credit to be spent one at a time.
    fn task_wait_notification(t: Timeout) -> bool;

    /// Pointer-width sibling of [`Rtos::queue_create`] and friends.
    ///
    /// A parallel triple rather than widening the existing one: `main_queue`
    /// and `background_pool` deliberately pack their work into a `u32` word,
    /// and widening would cost them memory on every entry for no gain. What
    /// these three carry instead is a *pointer*, which `u32` would silently
    /// truncate on a 64-bit host — so the simulator, not the device, is the
    /// reason the distinction has to exist in the seam rather than in a cast
    /// at the call site.
    fn queue_create_ptr(depth: usize) -> RawQueue;
    fn queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool;
    fn queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize>;

    /// Create a *recursive* mutex — Java monitors re-enter.
    fn mutex_recursive_create() -> Option<RawMutex>;
    fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool;
    fn mutex_recursive_unlock(m: RawMutex);

    fn sem_binary_create() -> RawSem;
    fn sem_give(s: RawSem);
    fn sem_take(s: RawSem, t: Timeout) -> bool;

    /// Start the periodic UI tick, or unpause it if already started.
    /// Singleton: one tick source per process.
    fn tick_timer_start(period_ms: u32, cb: fn());
    /// Stop posting ticks but stay ready to resume. The activity loop calls
    /// this before entering display sleep, so a platform should genuinely
    /// quiesce its timer here rather than filter at the callback — that is
    /// what lets the chip reach a deeper idle state.
    fn tick_timer_pause();
    fn tick_timer_resume();
    /// Tear the tick source down. Distinct from `pause`: a platform backing
    /// the tick with a thread must join it here, or an app reload leaks one
    /// tick thread per run.
    fn tick_timer_stop();

    /// Block the calling task.
    fn delay_ms(ms: u32);
}

extern "Rust" {
    fn __pd_rtos_spawn(spec: &TaskSpec, body: Box<dyn FnOnce() + Send>) -> bool;
    fn __pd_rtos_queue_create(depth: usize) -> RawQueue;
    fn __pd_rtos_queue_send(q: RawQueue, word: u32, t: Timeout) -> bool;
    fn __pd_rtos_queue_recv(q: RawQueue, t: Timeout) -> Option<u32>;
    fn __pd_rtos_task_current() -> RawTask;
    fn __pd_rtos_scheduler_running() -> bool;
    fn __pd_rtos_task_notify(t: RawTask);
    fn __pd_rtos_task_wait_notification(t: Timeout) -> bool;
    fn __pd_rtos_queue_create_ptr(depth: usize) -> RawQueue;
    fn __pd_rtos_queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool;
    fn __pd_rtos_queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize>;
    fn __pd_rtos_mutex_recursive_create() -> Option<RawMutex>;
    fn __pd_rtos_mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool;
    fn __pd_rtos_mutex_recursive_unlock(m: RawMutex);
    fn __pd_rtos_sem_binary_create() -> RawSem;
    fn __pd_rtos_sem_give(s: RawSem);
    fn __pd_rtos_sem_take(s: RawSem, t: Timeout) -> bool;
    fn __pd_rtos_tick_timer_start(period_ms: u32, cb: fn());
    fn __pd_rtos_tick_timer_pause();
    fn __pd_rtos_tick_timer_resume();
    fn __pd_rtos_tick_timer_stop();
    fn __pd_rtos_delay_ms(ms: u32);
}

/// Spawn a task running `body`. See [`Rtos::spawn`]; `false` means the
/// platform declined or could not create it.
pub fn spawn(spec: &TaskSpec, body: Box<dyn FnOnce() + Send>) -> bool {
    unsafe { __pd_rtos_spawn(spec, body) }
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
/// Handle of the calling task; 0 if there is no task context. See
/// [`Rtos::task_current`].
pub fn task_current() -> RawTask {
    unsafe { __pd_rtos_task_current() }
}
/// Whether the scheduler is running. See [`Rtos::scheduler_running`].
pub fn scheduler_running() -> bool {
    unsafe { __pd_rtos_scheduler_running() }
}
/// Wake `t`. See [`Rtos::task_notify`]; a no-op when `t` is 0.
pub fn task_notify(t: RawTask) {
    unsafe { __pd_rtos_task_notify(t) }
}
/// Block until notified. See [`Rtos::task_wait_notification`]; false means
/// the timeout elapsed.
pub fn task_wait_notification(t: Timeout) -> bool {
    unsafe { __pd_rtos_task_wait_notification(t) }
}
pub fn queue_create_ptr(depth: usize) -> RawQueue {
    unsafe { __pd_rtos_queue_create_ptr(depth) }
}
pub fn queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool {
    unsafe { __pd_rtos_queue_send_ptr(q, val, t) }
}
pub fn queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize> {
    unsafe { __pd_rtos_queue_recv_ptr(q, t) }
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
pub fn tick_timer_stop() {
    unsafe { __pd_rtos_tick_timer_stop() }
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
            use $crate::rtos::{RawMutex, RawQueue, RawSem, RawTask, TaskSpec, Timeout};

            #[no_mangle]
            extern "Rust" fn __pd_rtos_spawn(
                spec: &TaskSpec,
                body: ::alloc::boxed::Box<dyn FnOnce() + Send>,
            ) -> bool {
                <$t as $crate::rtos::Rtos>::spawn(spec, body)
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
            extern "Rust" fn __pd_rtos_task_current() -> RawTask {
                <$t as $crate::rtos::Rtos>::task_current()
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_scheduler_running() -> bool {
                <$t as $crate::rtos::Rtos>::scheduler_running()
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_task_notify(t: RawTask) {
                <$t as $crate::rtos::Rtos>::task_notify(t)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_task_wait_notification(t: Timeout) -> bool {
                <$t as $crate::rtos::Rtos>::task_wait_notification(t)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_queue_create_ptr(depth: usize) -> RawQueue {
                <$t as $crate::rtos::Rtos>::queue_create_ptr(depth)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool {
                <$t as $crate::rtos::Rtos>::queue_send_ptr(q, val, t)
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize> {
                <$t as $crate::rtos::Rtos>::queue_recv_ptr(q, t)
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
            extern "Rust" fn __pd_rtos_tick_timer_stop() {
                <$t as $crate::rtos::Rtos>::tick_timer_stop()
            }
            #[no_mangle]
            extern "Rust" fn __pd_rtos_delay_ms(ms: u32) {
                <$t as $crate::rtos::Rtos>::delay_ms(ms)
            }
        };
    };
}
