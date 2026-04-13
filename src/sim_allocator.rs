//! Capped global allocator for the host simulator.
//!
//! Wraps the system allocator with a configurable byte limit, simulating the
//! constrained FreeRTOS heap on RP2040/RP2350.  Set the `PICODROID_HEAP_LIMIT_KB`
//! environment variable at runtime to enforce a cap (e.g. `128` for 128 KB).
//! When unset, allocations are unlimited.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

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

unsafe impl GlobalAlloc for CappedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let limit = heap_limit();

        // Relaxed is fine — sim is single-threaded for JVM work.
        let prev = self.allocated.fetch_add(size, Ordering::Relaxed);
        if prev + size > limit {
            // Over budget — undo and return null (triggers Rust OOM).
            self.allocated.fetch_sub(size, Ordering::Relaxed);
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
        self.allocated.fetch_sub(layout.size(), Ordering::Relaxed);
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
}
