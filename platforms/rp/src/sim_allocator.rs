// SPDX-License-Identifier: GPL-3.0-only
//! Capped global allocator for the host simulator.
//!
//! Wraps the system allocator with a configurable byte limit, simulating the
//! constrained FreeRTOS heap on RP2040/RP2350.  Set the `PICODROID_HEAP_LIMIT_KB`
//! environment variable at runtime to enforce a cap (e.g. `128` for 128 KB).
//! When unset, allocations are unlimited.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

#[global_allocator]
pub static GLOBAL: CappedAllocator = CappedAllocator::new();

thread_local! {
    /// Nonzero while the current thread is inside a [`bypass`] region. Reads and
    /// writes must never re-enter the global allocator, so this is
    /// `const`-initialized — a lazily-initialized TLS would allocate on first
    /// touch and deadlock inside `alloc`. `Cell<u32>` has no destructor, so it
    /// also stays accessible during thread teardown.
    static BYPASS_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// RAII guard returned by [`bypass`]. While at least one guard is alive on the
/// current thread, `CappedAllocator` neither counts allocations against the
/// simulated heap nor enforces the limit on them.
///
/// This exists to exclude *host-only* artifacts — allocations that have no
/// counterpart in the MCU JVM heap — from the simulated budget. The motivating
/// case is the minifb window's full-screen backing buffer: at `Scale::X2` a
/// 240×240 display becomes a 480×480 `Vec<u32>` (≈900 KB), which dwarfs a
/// realistic MCU heap and would OOM the sim before the app runs. On real
/// hardware there is no such framebuffer — LVGL renders into a small banded
/// buffer streamed to the panel over SPI — so charging it to the JVM heap cap
/// mismodels the device.
///
/// Balance rule: allocations made inside a bypass region must also be *freed*
/// inside a bypass region (or leaked until process exit), so that `dealloc`
/// never decrements the counter for bytes it never added. minifb satisfies
/// this — its buffers are (re)allocated and freed entirely within the guarded
/// `Window::new` / `update_with_buffer` calls, and the final window is leaked
/// at exit. `dealloc` also saturates at zero as a defensive backstop.
#[must_use]
pub struct BypassGuard(());

impl Drop for BypassGuard {
    fn drop(&mut self) {
        BYPASS_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Enter a heap-cap bypass region on the current thread until the returned
/// guard is dropped. See [`BypassGuard`]. Nesting is supported (depth-counted).
pub fn bypass() -> BypassGuard {
    BYPASS_DEPTH.with(|d| d.set(d.get() + 1));
    BypassGuard(())
}

fn bypass_active() -> bool {
    BYPASS_DEPTH.with(Cell::get) != 0
}

/// Snapshot the global allocator. Returns `(current_bytes, peak_bytes, limit_bytes)`.
pub fn heap_stats() -> (usize, usize, usize) {
    GLOBAL.heap_stats()
}

/// Per-phase heap checkpoint for the sim. Prints the byte delta since the
/// previous call alongside the current and peak values so transient bloat
/// (allocations freed before the next checkpoint) is visible too.
pub fn checkpoint(label: &str) {
    static PREV: AtomicUsize = AtomicUsize::new(0);
    let (cur, peak, _) = heap_stats();
    let prev = PREV.swap(cur, Ordering::Relaxed);
    let delta = cur as i64 - prev as i64;
    println!(
        "[sim] heap phase: {:<22} delta {:+8} B (cur {:>8} B, peak {:>8} B)",
        label, delta, cur, peak,
    );
}

pub struct CappedAllocator {
    allocated: AtomicUsize,
    peak: AtomicUsize,
}

impl CappedAllocator {
    pub const fn new() -> Self {
        Self {
            allocated: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    /// Returns (current_bytes, peak_bytes, limit_bytes).
    pub fn heap_stats(&self) -> (usize, usize, usize) {
        (
            self.allocated.load(Ordering::Relaxed),
            self.peak.load(Ordering::Relaxed),
            heap_limit(),
        )
    }
}

/// Read the heap limit once per process.  Returns `usize::MAX` if unset.
///
/// Uses libc `getenv` instead of `std::env::var` to avoid allocating a String
/// inside the global allocator (which would cause reentrant deadlock).
fn heap_limit() -> usize {
    use std::sync::atomic::AtomicUsize;
    // 0 = not yet initialized, 1..MAX = limit, MAX = unlimited.
    static LIMIT: AtomicUsize = AtomicUsize::new(0);

    let cached = LIMIT.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }

    let val = parse_env_limit();
    LIMIT.store(val, Ordering::Relaxed);
    val
}

/// Parse `PICODROID_HEAP_LIMIT_KB` from the environment without allocating.
fn parse_env_limit() -> usize {
    let name = b"PICODROID_HEAP_LIMIT_KB\0";
    let ptr = unsafe { libc::getenv(name.as_ptr() as *const libc::c_char) };
    if ptr.is_null() {
        return usize::MAX;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
    match cstr.to_str().ok().and_then(|s| s.parse::<usize>().ok()) {
        Some(kb) => kb * 1024,
        None => usize::MAX,
    }
}

/// Stack-only writer for emitting OOM diagnostics without re-entering the
/// allocator (println/eprintln would recurse since the failing allocation
/// arrives via the global allocator).
struct StackWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}
impl core::fmt::Write for StackWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        let n = b.len().min(self.buf.len().saturating_sub(self.pos));
        self.buf[self.pos..self.pos + n].copy_from_slice(&b[..n]);
        self.pos += n;
        Ok(())
    }
}

unsafe impl GlobalAlloc for CappedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Host-only allocation (see `bypass`): serve it uncounted and uncapped.
        if bypass_active() {
            return unsafe { System.alloc(layout) };
        }

        let size = layout.size();
        let limit = heap_limit();

        // Relaxed is fine — sim is single-threaded for JVM work.
        let prev = self.allocated.fetch_add(size, Ordering::Relaxed);
        if prev + size > limit {
            // Over budget — undo and return null (triggers Rust OOM).
            self.allocated.fetch_sub(size, Ordering::Relaxed);
            let peak = self.peak.load(Ordering::Relaxed);
            let mut buf = [0u8; 256];
            let pos = {
                let mut w = StackWriter {
                    buf: &mut buf,
                    pos: 0,
                };
                use core::fmt::Write;
                let _ = writeln!(
                    w,
                    "[sim] OOM: tried {} B, allocated {} B, peak {} B, limit {} B",
                    size, prev, peak, limit,
                );
                w.pos
            };
            unsafe {
                libc::write(2, buf.as_ptr() as *const _, pos);
            }
            return std::ptr::null_mut();
        }

        let ptr = unsafe { System.alloc(layout) };
        if ptr.is_null() {
            // System allocator failed — undo our accounting.
            self.allocated.fetch_sub(size, Ordering::Relaxed);
        } else {
            // Update peak high-water mark.
            let current = prev + size;
            self.peak.fetch_max(current, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        // A free inside a bypass region matches a bypassed (uncounted) alloc, so
        // leave the counter alone. `fetch_update` saturates at zero as a
        // backstop against ever underflowing into a permanent spurious OOM.
        if bypass_active() {
            return;
        }
        let size = layout.size();
        let _ = self
            .allocated
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                Some(cur.saturating_sub(size))
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_accuracy() {
        let alloc = CappedAllocator::new();
        let layout = Layout::from_size_align(256, 8).unwrap();

        // Allocate a block.
        let ptr = unsafe { GlobalAlloc::alloc(&alloc, layout) };
        assert!(!ptr.is_null());
        let (current, peak, _) = alloc.heap_stats();
        assert_eq!(current, 256);
        assert_eq!(peak, 256);

        // Deallocate.
        unsafe { GlobalAlloc::dealloc(&alloc, ptr, layout) };
        let (current, peak, _) = alloc.heap_stats();
        assert_eq!(current, 0);
        assert_eq!(peak, 256); // peak unchanged
    }

    #[test]
    fn peak_tracking() {
        let alloc = CappedAllocator::new();
        let layout_a = Layout::from_size_align(512, 8).unwrap();
        let layout_b = Layout::from_size_align(128, 8).unwrap();

        let ptr_a = unsafe { GlobalAlloc::alloc(&alloc, layout_a) };
        assert!(!ptr_a.is_null());
        // peak = 512

        let ptr_b = unsafe { GlobalAlloc::alloc(&alloc, layout_b) };
        assert!(!ptr_b.is_null());
        // peak = 640

        unsafe { GlobalAlloc::dealloc(&alloc, ptr_a, layout_a) };
        // current = 128, peak still 640

        let (current, peak, _) = alloc.heap_stats();
        assert_eq!(current, 128);
        assert_eq!(peak, 640);

        unsafe { GlobalAlloc::dealloc(&alloc, ptr_b, layout_b) };
    }

    #[test]
    fn bypass_excludes_allocation_from_accounting() {
        let alloc = CappedAllocator::new();
        let layout = Layout::from_size_align(4096, 8).unwrap();

        // Inside a bypass region an allocation succeeds but is not counted —
        // this models minifb's host-only window buffer, which must not consume
        // the simulated JVM heap budget. Freeing it while still bypassed leaves
        // the counter balanced at zero.
        {
            let _bypass = bypass();
            let p = unsafe { GlobalAlloc::alloc(&alloc, layout) };
            assert!(!p.is_null());
            let (current, peak, _) = alloc.heap_stats();
            assert_eq!(current, 0, "bypassed alloc must not be counted");
            assert_eq!(peak, 0, "bypassed alloc must not move the high-water mark");
            unsafe { GlobalAlloc::dealloc(&alloc, p, layout) };
            assert_eq!(alloc.heap_stats().0, 0, "bypassed free is balanced");
        }

        // The guard is dropped, so accounting resumes: a normal alloc is counted.
        let counted = unsafe { GlobalAlloc::alloc(&alloc, layout) };
        assert!(!counted.is_null());
        assert_eq!(alloc.heap_stats().0, 4096, "unbypassed alloc is counted");
        unsafe { GlobalAlloc::dealloc(&alloc, counted, layout) };
        assert_eq!(alloc.heap_stats().0, 0);
    }

    #[test]
    fn dealloc_saturates_at_zero() {
        // Defensive backstop: if a bypass-allocated pointer is ever freed
        // outside a bypass region, `dealloc` must floor the counter at zero
        // rather than wrap around into a permanent spurious OOM.
        let alloc = CappedAllocator::new();
        let layout = Layout::from_size_align(4096, 8).unwrap();

        let ptr = {
            let _bypass = bypass();
            unsafe { GlobalAlloc::alloc(&alloc, layout) }
        };
        assert!(!ptr.is_null());
        assert_eq!(alloc.heap_stats().0, 0);

        // Counter is 0; freeing 4096 unbypassed would underflow without the floor.
        unsafe { GlobalAlloc::dealloc(&alloc, ptr, layout) };
        assert_eq!(alloc.heap_stats().0, 0, "saturating_sub floors at zero");
    }
}
