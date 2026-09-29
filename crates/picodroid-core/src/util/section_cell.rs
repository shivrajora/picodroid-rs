// SPDX-License-Identifier: GPL-3.0-only
//! State that is only ever touched with the scheduler suspended.
//!
//! The thread table, the monitor store and their like are shared by every
//! JVM task, and each access sits inside an [`AtomicSection`]: no other task
//! on the core can run until the guard drops. That used to be a comment on
//! an `unsafe fn table()`. Here it is the signature — [`SectionCell::get`]
//! wants the guard, exclusively, and the reference it returns cannot
//! outlive it.

#![deny(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]

use core::cell::UnsafeCell;

use pico_jvm::atomic_section::AtomicSection;

pub struct SectionCell<T>(UnsafeCell<T>);

// SAFETY: `SectionCell::new` is `unsafe`; its contract confines the value to
// core-0 tasks, and `get` admits one of them at a time.
unsafe impl<T> Sync for SectionCell<T> {}

impl<T> SectionCell<T> {
    /// # Safety
    ///
    /// Every access must come from a task pinned to core 0 — an
    /// `AtomicSection` suspends that core's scheduler and stops neither an
    /// interrupt handler nor core 1 — and code holding the reference
    /// [`get`](Self::get) returned must not call anything that opens a
    /// section of its own to reach the same cell.
    pub const unsafe fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    /// The value, for as long as `held` is borrowed.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn get<'a>(&'a self, held: &'a mut AtomicSection) -> &'a mut T {
        let _ = held;
        // SAFETY: the scheduler is suspended while `held` lives, so no other
        // core-0 task runs; the exclusive borrow of `held` makes this the
        // only reference handed out under this guard; `new`'s contract
        // covers the rest.
        unsafe { &mut *self.0.get() }
    }
}
