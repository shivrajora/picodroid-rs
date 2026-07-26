// SPDX-License-Identifier: GPL-3.0-only
//! Platform hooks — the residue that is neither HAL nor RTOS.
//!
//! Four things shared code needs that no peripheral trait covers: whether a
//! debugger has asked the JVM to stop, the JVM's shared heap (a `static` that
//! must live in the binary crate), and the simulator's heap-accounting
//! controls.
//!
//! Two candidates were deliberately *not* put here. Task stack sizing and the
//! simulator's thread-spawn heap charge both looked like hooks, but they are
//! only ever consumed by the platform's own [`crate::rtos::Rtos::spawn`]
//! implementation — which is platform code already. Routing them through the
//! seam would have exported the boot budget for no reason. See the design
//! doc's amendments.

/// Native (non-JVM) heap usage, as the memory monitor reports it.
///
/// Fields mirror FreeRTOS's `HeapStats_t` subset that `mem_diag` prints; the
/// simulator fills them from its `heap_4` port so both paths format alike.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeHeapStats {
    pub free_bytes: usize,
    pub min_ever_free_bytes: usize,
    pub alloc_count: usize,
    pub free_count: usize,
}

/// Hooks a platform must provide.
pub trait PlatformHooks {
    /// Has a debug bridge asked the JVM to stop? Polled at safepoints.
    /// Simulator and test builds return false.
    fn stop_requested() -> bool;

    /// Enter a region whose allocations are *not* charged to the simulated
    /// device heap. No-op on hardware.
    ///
    /// This exists because the simulator's heap cap models a device arena,
    /// and host-only allocations (notably the ~900 KB minifb window buffer)
    /// would otherwise blow a realistic cap the moment the window opens.
    fn heap_bypass_enter();
    fn heap_bypass_exit();

    /// Record a labelled heap checkpoint for the memory monitor. No-op on
    /// hardware and when diagnostics are off.
    fn heap_checkpoint(label: &str);

    /// Native heap statistics for the memory monitor.
    fn native_heap_stats() -> NativeHeapStats;
}

extern "Rust" {
    fn __pd_host_stop_requested() -> bool;
    fn __pd_host_heap_bypass_enter();
    fn __pd_host_heap_bypass_exit();
    fn __pd_host_heap_checkpoint(label: &str);
    fn __pd_host_native_heap_stats() -> NativeHeapStats;
}

/// Has a debug bridge asked the JVM to stop?
pub fn stop_requested() -> bool {
    unsafe { __pd_host_stop_requested() }
}

/// Record a labelled heap checkpoint.
pub fn heap_checkpoint(label: &str) {
    unsafe { __pd_host_heap_checkpoint(label) }
}

/// Native heap statistics.
pub fn native_heap_stats() -> NativeHeapStats {
    unsafe { __pd_host_native_heap_stats() }
}

/// RAII guard for a heap-accounting bypass region.
///
/// Mirrors the binary crate's own `BypassGuard`, so moved code keeps the
/// `let _g = heap_bypass();` shape it already had.
#[must_use = "the bypass ends when the guard drops; bind it to a variable"]
pub struct BypassGuard(());

impl Drop for BypassGuard {
    fn drop(&mut self) {
        unsafe { __pd_host_heap_bypass_exit() }
    }
}

/// Open a region whose allocations bypass the simulated device heap cap.
pub fn heap_bypass() -> BypassGuard {
    unsafe { __pd_host_heap_bypass_enter() }
    BypassGuard(())
}

/// Register the platform's [`PlatformHooks`] implementation.
#[macro_export]
macro_rules! set_platform_hooks {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_host_stop_requested() -> bool {
                <$t as $crate::host::PlatformHooks>::stop_requested()
            }
            #[no_mangle]
            extern "Rust" fn __pd_host_heap_bypass_enter() {
                <$t as $crate::host::PlatformHooks>::heap_bypass_enter()
            }
            #[no_mangle]
            extern "Rust" fn __pd_host_heap_bypass_exit() {
                <$t as $crate::host::PlatformHooks>::heap_bypass_exit()
            }
            #[no_mangle]
            extern "Rust" fn __pd_host_heap_checkpoint(label: &str) {
                <$t as $crate::host::PlatformHooks>::heap_checkpoint(label)
            }
            #[no_mangle]
            extern "Rust" fn __pd_host_native_heap_stats() -> $crate::host::NativeHeapStats {
                <$t as $crate::host::PlatformHooks>::native_heap_stats()
            }
        };
    };
}
