// SPDX-License-Identifier: GPL-3.0-only
//! A value stored once, early, and read from anywhere afterwards.
//!
//! The drivers keep their kernel objects — a transfer's completion
//! semaphore, a bus lock, the queue an interrupt feeds — in statics that an
//! init function fills and that interrupt handlers and tasks then read
//! without a lock. Each module used to carry its own
//! `XxxCell(UnsafeCell<Option<T>>)` with an `unsafe impl Sync` and an
//! `unsafe` block at every read. [`SetOnce`] is that pattern written once:
//! the store is the `unsafe` part, because only its caller can say that
//! nothing reads the cell while it writes; the read is a plain load of what
//! was stored, and is safe.
//!
//! No lock and no atomic: a read compiles to the same load the hand-written
//! cells did, which matters inside interrupt handlers. That is also why
//! [`SetOnce::set`] carries the whole proof — it must run before anything
//! that could read the cell is able to (before the interrupt that reads it
//! is unmasked, before the tasks that read it exist), or while every
//! possible reader is otherwise held off.

#![deny(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]

use core::cell::UnsafeCell;

pub struct SetOnce<T>(UnsafeCell<Option<T>>);

// SAFETY: a `&SetOnce<T>` shared between threads hands out `&T` (`get`),
// which needs `T: Sync`, and lets one thread store a `T` another later reads
// or drops, which needs `T: Send`. The store itself is `set`'s `unsafe`
// contract: it never runs while another thread reads the cell.
unsafe impl<T: Send + Sync> Sync for SetOnce<T> {}

impl<T> SetOnce<T> {
    /// An empty cell.
    pub const fn new() -> Self {
        Self(UnsafeCell::new(None))
    }

    /// Store `value`.
    ///
    /// # Safety
    ///
    /// - No [`get`](Self::get) or `set` of this cell may run while this call
    ///   does: not on another task, not on the other core, and not in an
    ///   interrupt handler that can preempt the caller. In practice the
    ///   store happens before any reader can exist — before the scheduler
    ///   starts, before the reading task is created, or before the reading
    ///   interrupt is unmasked — or under a guard that excludes them.
    /// - If the cell already holds a value, no reference an earlier `get`
    ///   returned may still be in use: storing drops the old value. Every
    ///   caller here stores into an empty cell.
    #[inline(always)]
    pub unsafe fn set(&self, value: T) {
        // SAFETY: the caller guarantees that no other access to the cell is
        // in progress and that no reference into the old value is live, so
        // this is the only reference to the slot.
        unsafe { *self.0.get() = Some(value) };
    }

    /// The stored value, or `None` before [`set`](Self::set).
    #[inline(always)]
    pub fn get(&self) -> Option<&T> {
        // SAFETY: `set` is the only writer, and its contract keeps it from
        // running while this read does or while the reference handed out
        // here is in use (it never replaces a value a reader holds).
        unsafe { (*self.0.get()).as_ref() }
    }
}

impl<T> Default for SetOnce<T> {
    fn default() -> Self {
        Self::new()
    }
}
