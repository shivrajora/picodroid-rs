// SPDX-License-Identifier: GPL-3.0-only
//! A host-test allocator with a per-thread budget, so the out-of-memory
//! paths can be tested instead of only reasoned about.
//!
//! The 2026-09-13 QA round found several allocations on Java-reachable paths
//! that aborted the firmware (a board reset) where an `OutOfMemoryError` was
//! due: the formatter, file streams, interpreter frames, and interning a
//! dynamic string. The fixes made each of them fallible, and a fallible
//! allocation is only testable if an allocation can be made to fail.
//!
//! [`with_budget`] models a fixed heap **on the calling thread**: `bytes`
//! live at once, charged on allocation and refunded on free, and past the
//! cap the allocator hands back null — which `try_reserve` and friends
//! report as an error rather than aborting. Refunding is what makes a
//! collection observable: an interpreter that frees its garbage and retries
//! gets served, one that gives up at the first refusal does not. Every other
//! thread — and every test that does not ask for a cap — allocates normally,
//! so the suite still runs in parallel.
use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use std::alloc::System;
use std::cell::Cell;

std::thread_local! {
    /// Bytes this thread may still allocate; `NO_CAP` means "unlimited".
    static BUDGET: Cell<usize> = const { Cell::new(NO_CAP) };
}

const NO_CAP: usize = usize::MAX;

struct Capped;

impl Capped {
    /// Refund `size` to this thread's budget, as freeing memory does on a
    /// real heap. A thread that was never capped stays uncapped.
    fn refund(size: usize) {
        let _ = BUDGET.try_with(|b| {
            let left = b.get();
            if left != NO_CAP {
                b.set(left.saturating_add(size));
            }
        });
    }

    /// Charge `size` against this thread's budget; `false` means refuse.
    /// A thread whose TLS is gone (teardown) is never capped.
    fn charge(size: usize) -> bool {
        BUDGET
            .try_with(|b| {
                let left = b.get();
                if left == NO_CAP {
                    return true;
                }
                if size > left {
                    return false;
                }
                b.set(left - size);
                true
            })
            .unwrap_or(true)
    }
}

// SAFETY: every arm forwards to `System`, which upholds the `GlobalAlloc`
// contract; the cap only turns an allocation into the null return that
// `GlobalAlloc` already allows.
unsafe impl GlobalAlloc for Capped {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !Self::charge(layout.size()) {
            return ptr::null_mut();
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        Self::refund(layout.size());
        unsafe { System.dealloc(p, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !Self::charge(layout.size()) {
            return ptr::null_mut();
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Only growth is charged; a shrink refunds the difference.
        if new_size > layout.size() {
            if !Self::charge(new_size - layout.size()) {
                return ptr::null_mut();
            }
        } else {
            Self::refund(layout.size() - new_size);
        }
        unsafe { System.realloc(p, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Capped = Capped;

/// Run `f` with at most `bytes` of further allocation on this thread, then
/// restore the previous budget (so a nested call, or a test that panics,
/// leaves the thread uncapped again).
pub(crate) fn with_budget<R>(bytes: usize, f: impl FnOnce() -> R) -> R {
    struct Restore(usize);
    impl Drop for Restore {
        fn drop(&mut self) {
            BUDGET.with(|b| b.set(self.0));
        }
    }
    let prev = BUDGET.with(|b| b.replace(bytes));
    let _restore = Restore(prev);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn a_budgeted_thread_refuses_an_allocation_past_its_cap() {
        with_budget(1024, || {
            let mut v: Vec<u8> = Vec::new();
            assert!(v.try_reserve_exact(64).is_ok());
            assert!(
                v.try_reserve_exact(1 << 20).is_err(),
                "a request past the cap must fail softly"
            );
        });
    }

    #[test]
    fn freeing_gives_the_budget_back() {
        with_budget(4096, || {
            let mut a: Vec<u8> = Vec::new();
            a.try_reserve_exact(2048).expect("first fits");
            let mut b: Vec<u8> = Vec::new();
            assert!(b.try_reserve_exact(4096).is_err(), "both do not fit");
            drop(a);
            b.try_reserve_exact(3072)
                .expect("fits once the first is freed");
        });
    }

    #[test]
    fn the_budget_is_restored_after_the_scope() {
        with_budget(256, || {});
        let mut v: Vec<u8> = Vec::new();
        assert!(v.try_reserve_exact(1 << 16).is_ok());
    }

    #[test]
    fn an_uncapped_thread_is_never_refused() {
        let mut v: Vec<u8> = Vec::new();
        assert!(v.try_reserve_exact(1 << 20).is_ok());
    }
}
