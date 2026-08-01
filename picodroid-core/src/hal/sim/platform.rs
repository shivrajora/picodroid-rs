// SPDX-License-Identifier: GPL-3.0-only
//! Ready-made simulator registration, so a family does not write one.
//!
//! A family's HAL impls already serve both arms — they delegate through its
//! `hal` module, whose `mod chip` selects this crate's [`super`] in simulator
//! builds — so the HAL needs nothing here. What every family *did* duplicate
//! is the other two registrations: an `Rtos` backed by host threads (351
//! lines of it, which had already drifted into a near-copy inside this crate)
//! and the simulator half of [`crate::host::PlatformHooks`].
//!
//! [`crate::register_sim_platform`] generates both. The family supplies the
//! two leaves that are genuinely its own — see the macro's docs.

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

/// Register this crate's simulator as the platform's RTOS and hooks.
///
/// Covers the two registrations a simulator build would otherwise hand-write.
/// The HAL is not among them: a family's own HAL impls already reach the
/// shared simulator through its `hal` module's `mod chip` dispatch, so one set
/// of impls serves hardware and simulator alike.
///
/// Two parameters, both genuinely the family's own:
///
/// - `gc_roots` — the family's GC root registration. Required, not defaulted,
///   for the same reason [`crate::host::PlatformHooks::register_gc_roots`] is:
///   a family with no native modules holding Java references should write that
///   down deliberately rather than have the question go unasked.
/// - `charge_task_spawn` — bill the family's boot budget for the stack and TCB
///   the device would have allocated for this task, and return that stack size
///   in bytes. A parameter rather than a hook because the boot budget is
///   chip-gated platform data; routing it through the seam would export it to
///   shared code for no reason. The return value is how stack *sizing* stays
///   with the platform too (`crate::rtos`): the FreeRTOS backing creates a real
///   task from the same number it charges, so the two cannot disagree. The
///   `cargo test` backing calls it only for the `Thread.start` it refuses, and
///   discards the number.
/// - `release_task_spawn` — undo that charge when the task's body returns.
///   Only the kernel backing has an exit to call it from; under the `cargo
///   test` backing nothing ran, so nothing is released.
///
/// ```ignore
/// #[cfg(any(test, feature = "sim"))]
/// picodroid_core::register_sim_platform! {
///     gc_roots = crate::gc_root_registration::register_all,
///     charge_task_spawn = crate::boot_budget::charge_task_spawn,
///     release_task_spawn = crate::boot_budget::release_task_spawn,
/// }
/// ```
#[macro_export]
macro_rules! register_sim_platform {
    (
        gc_roots = $gc_roots:path,
        charge_task_spawn = $charge:path,
        release_task_spawn = $release:path $(,)?
    ) => {
        const _: () = {
            use $crate::hal::sim::rtos;
            use $crate::rtos::{RawMutex, RawQueue, RawSem, RawTask, TaskSpec, Timeout};

            /// The shared simulator, as the platform registers it.
            struct SimPlatform;

            unsafe impl $crate::rtos::Rtos for SimPlatform {
                fn spawn(spec: &TaskSpec, body: ::alloc::boxed::Box<dyn FnOnce() + Send>) -> bool {
                    rtos::spawn(spec, body, $charge, $release)
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
    };
}
