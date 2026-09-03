// SPDX-License-Identifier: GPL-3.0-only
//! Ready-made simulator registration, so a family does not write one.
//!
//! A family's HAL impls already serve both arms — they delegate through its
//! `hal` module, whose `mod chip` selects this crate's [`super`] in simulator
//! builds — so the HAL needs nothing here. What every family *did* duplicate
//! is everything else: an `Rtos` backed by the hosted kernel, the simulator
//! half of [`crate::host::PlatformHooks`], the boot-budget bookkeeping, and
//! the simulator's `main`.
//!
//! [`crate::register_sim_platform`] generates all of it. The family supplies
//! the three leaves that are genuinely its own — see the macro's docs.

use crate::host::NativeHeapStats;

/// Native heap statistics as the simulator sees them.
///
/// Two independent sources: the byte meter that models the device arena (used
/// vs. cap), and the heap_4 mirror that reports block-level detail. Uncapped
/// runs (`-l 0`) have no arena, so free is derived from the meter and
/// fragmentation reports as unavailable — the monitor reads a zero largest
/// block as "unknown" rather than "fully fragmented".
pub fn native_heap_stats() -> NativeHeapStats {
    let (cur, _peak, limit) = super::allocator::heap_stats();
    match super::allocator::heap4_stats() {
        Some(h) => NativeHeapStats {
            used_bytes: cur,
            free_bytes: h.free_bytes as usize,
            min_ever_free_bytes: h.min_ever_free_bytes as usize,
            largest_free_block: h.largest_free_block as usize,
        },
        None => NativeHeapStats {
            used_bytes: cur,
            free_bytes: limit.saturating_sub(cur),
            min_ever_free_bytes: limit.saturating_sub(cur),
            largest_free_block: 0,
        },
    }
}

/// Register this crate's simulator as the platform's RTOS and hooks, and
/// generate the simulator's `main`.
///
/// Covers the registrations a simulator build would otherwise hand-write.
/// The HAL is not among them: a family's own HAL impls already reach the
/// shared simulator through its `hal` module's `mod chip` dispatch, so one set
/// of impls serves hardware and simulator alike.
///
/// Three parameters, all genuinely the family's own:
///
/// - `gc_roots` — the family's GC root registration, a `fn()`. Required, not
///   defaulted, for the same reason [`crate::host::PlatformHooks::register_gc_roots`]
///   is: a family with no native modules holding Java references should write
///   that down deliberately rather than have the question go unasked.
/// - `boot_budget` — a `static` [`super::boot_budget::BootBudgetModel`]: the
///   tasks the device creates at boot, their stack sizes, and the TCB and
///   queue estimates. Chip-gated platform data, so it crosses as a parameter
///   rather than as a `PlatformHooks` method every device family would also
///   have to stub. The shared engine charges the arena from it, sizes every
///   kernel task from it (so the charge and the allocation are one number),
///   and asserts at the end of boot that the two routes reconcile.
/// - `run_app` — a `fn()` that runs the family's app to completion. The
///   family's, because where the app bytes come from is a property of its
///   flash layout and build.
///
/// The generated `sim_main()` is emitted under `#[cfg(feature = "sim")]` of
/// the *invoking* crate, so a family's simulator feature must be named `sim`
/// (every script already assumes it). `main.rs` calls it and nothing else.
///
/// ```ignore
/// #[cfg(any(test, feature = "sim"))]
/// picodroid_core::register_sim_platform! {
///     gc_roots    = crate::gc_root_registration::register_all,
///     boot_budget = crate::boot_budget::MODEL,
///     run_app     = crate::app::run_jvm,
/// }
///
/// #[cfg(feature = "sim")]
/// fn main() {
///     glue::sim_main()
/// }
/// ```
#[macro_export]
macro_rules! register_sim_platform {
    (
        gc_roots = $gc_roots:path,
        boot_budget = $boot_budget:path,
        run_app = $run_app:path $(,)?
    ) => {
        const _: () = {
            use $crate::hal::sim::{boot_budget, rtos};
            use $crate::rtos::{RawMutex, RawQueue, RawSem, RawTask, TaskSpec, Timeout};

            /// Bill the family's model for a task the kernel is creating, and
            /// report the stack size in bytes the device would have given it.
            fn charge(spec: &TaskSpec) -> u32 {
                boot_budget::charge_task_spawn(&$boot_budget, spec)
            }
            /// Undo [`charge`] when the task's body returns.
            fn release(spec: &TaskSpec) {
                boot_budget::release_task_spawn(&$boot_budget, spec)
            }

            /// The shared simulator, as the platform registers it.
            struct SimPlatform;

            unsafe impl $crate::rtos::Rtos for SimPlatform {
                fn spawn(spec: &TaskSpec, body: ::alloc::boxed::Box<dyn FnOnce() + Send>) -> bool {
                    rtos::spawn(spec, body, charge, release)
                }
                fn queue_create(depth: usize) -> RawQueue {
                    rtos::queue_create(depth)
                }
                fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool {
                    rtos::queue_send(q, word, t)
                }
                fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32> {
                    rtos::queue_recv(q, t)
                }
                fn task_current() -> RawTask {
                    rtos::task_current()
                }
                fn scheduler_running() -> bool {
                    rtos::scheduler_running()
                }
                fn task_notify(t: RawTask) {
                    rtos::task_notify(t)
                }
                fn task_wait_notification(t: Timeout) -> bool {
                    rtos::task_wait_notification(t)
                }
                fn queue_create_ptr(depth: usize) -> RawQueue {
                    rtos::queue_create_ptr(depth)
                }
                fn queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool {
                    rtos::queue_send_ptr(q, val, t)
                }
                fn queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize> {
                    rtos::queue_recv_ptr(q, t)
                }
                fn mutex_recursive_create() -> Option<RawMutex> {
                    rtos::mutex_recursive_create()
                }
                fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool {
                    rtos::mutex_recursive_lock(m, t)
                }
                fn mutex_recursive_unlock(m: RawMutex) {
                    rtos::mutex_recursive_unlock(m)
                }
                fn mutex_recursive_delete(m: RawMutex) {
                    rtos::mutex_recursive_delete(m)
                }
                fn sem_binary_create() -> RawSem {
                    rtos::sem_binary_create()
                }
                fn sem_give(s: RawSem) {
                    rtos::sem_give(s)
                }
                fn sem_take(s: RawSem, t: Timeout) -> bool {
                    rtos::sem_take(s, t)
                }
                fn tick_timer_start(period_ms: u32, cb: fn()) {
                    rtos::tick_timer_start(period_ms, cb)
                }
                fn tick_timer_pause() {
                    rtos::tick_timer_pause()
                }
                fn tick_timer_resume() {
                    rtos::tick_timer_resume()
                }
                fn tick_timer_stop() {
                    rtos::tick_timer_stop()
                }
                fn delay_ms(ms: u32) {
                    rtos::delay_ms(ms)
                }
            }

            impl $crate::host::PlatformHooks for SimPlatform {
                /// No debug bridge in the simulator, so nothing can ask the
                /// JVM to stop.
                fn stop_requested() -> bool {
                    false
                }
                fn heap_bypass_enter() {
                    $crate::hal::sim::allocator::bypass_enter()
                }
                fn heap_bypass_exit() {
                    $crate::hal::sim::allocator::bypass_exit()
                }
                fn heap_checkpoint(label: &str) {
                    $crate::hal::sim::allocator::checkpoint(label)
                }
                fn native_heap_stats() -> $crate::host::NativeHeapStats {
                    $crate::hal::sim::platform::native_heap_stats()
                }
                fn register_gc_roots() {
                    $gc_roots()
                }
            }

            $crate::set_rtos!(SimPlatform);
            $crate::set_platform_hooks!(SimPlatform);
        };

        /// The simulator's `main`: hand this family's boot-budget model and
        /// its app to `picodroid_core::sim_boot::main`, which owns the
        /// sequence.
        #[cfg(feature = "sim")]
        pub fn sim_main() {
            $crate::sim_boot::main(&$boot_budget, $run_app)
        }
    };
}
