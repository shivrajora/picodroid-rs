// SPDX-License-Identifier: GPL-3.0-only
//! A device-sized heap for the targets that run the JVM.
//!
//! On a board the JVM shares a few hundred kilobytes; on the host it has the
//! machine. A hostile class that allocates without end therefore ran into
//! OutOfMemoryError on the device and into the fuzzer's ten-second timeout
//! here (`new Object[4369][4369]` is 76 MB of arena and hours of marking
//! under ASan) — a "finding" no device can reproduce. Inside
//! [`with_budget`] this allocator refuses whatever would take the calling
//! thread past its budget, so the JVM meets the same wall it meets on a
//! board: a fallible reservation answers `None` and the interpreter throws,
//! and an infallible one aborts, which on a board is a reset and so a real
//! finding.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Bytes the current budget still allows; `None` outside a budget.
    static LEFT: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Take `n` bytes from the thread's budget; `false` if it cannot cover them.
fn take(n: usize) -> bool {
    LEFT.try_with(|left| match left.get() {
        None => true,
        Some(l) if l >= n => {
            left.set(Some(l - n));
            true
        }
        Some(_) => false,
    })
    .unwrap_or(true)
}

fn give(n: usize) {
    let _ = LEFT.try_with(|left| {
        if let Some(l) = left.get() {
            left.set(Some(l.saturating_add(n)));
        }
    });
}

/// The system allocator, metered against the thread's budget.
pub struct Budgeted;

// SAFETY: every method forwards to `System` with the caller's own layout
// and pointer, unchanged; the budget only decides whether to forward an
// allocation at all, and a refusal is the null pointer `GlobalAlloc`
// allows.
unsafe impl GlobalAlloc for Budgeted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !take(layout.size()) {
            return core::ptr::null_mut();
        }
        // SAFETY: the caller's contract, passed through.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !take(layout.size()) {
            return core::ptr::null_mut();
        }
        // SAFETY: as above.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        give(layout.size());
        // SAFETY: as above.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let grow = new_size.saturating_sub(layout.size());
        if !take(grow) {
            return core::ptr::null_mut();
        }
        // SAFETY: as above.
        let moved = unsafe { System.realloc(ptr, layout, new_size) };
        if moved.is_null() {
            give(grow);
        } else {
            give(layout.size().saturating_sub(new_size));
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Budgeted = Budgeted;

/// Run `f` with at most `bytes` of new heap on this thread.
pub fn with_budget<R>(bytes: usize, f: impl FnOnce() -> R) -> R {
    /// Ends the budget however `f` ends, a panic the replay catches included.
    struct End;
    impl Drop for End {
        fn drop(&mut self) {
            LEFT.with(|l| l.set(None));
        }
    }
    LEFT.with(|l| l.set(Some(bytes)));
    let _end = End;
    f()
}

#[cfg(test)]
mod tests {
    use super::with_budget;

    #[test]
    fn a_budget_refuses_past_its_size_and_frees_back_into_it() {
        with_budget(4096, || {
            let mut v: Vec<u8> = Vec::new();
            assert!(v.try_reserve_exact(8192).is_err());
            assert!(v.try_reserve_exact(2048).is_ok());
            drop(v);
            let mut w: Vec<u8> = Vec::new();
            assert!(w.try_reserve_exact(4000).is_ok());
        });
        // Outside a budget nothing is refused.
        let mut v: Vec<u8> = Vec::new();
        assert!(v.try_reserve_exact(1 << 20).is_ok());
    }
}
