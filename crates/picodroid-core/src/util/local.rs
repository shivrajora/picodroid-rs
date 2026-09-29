// SPDX-License-Identifier: GPL-3.0-only
//! State confined to the tasks that run Java.
//!
//! The widget layer keeps its registries and queues in statics, and a static
//! must be `Sync`. None of that state is shared in the sense `Sync` means:
//! it is written and read by JVM tasks only — the UI task, `Thread.start`
//! children, the background workers — and every one of them is pinned to
//! core 0 (`platforms/rp/src/task_affinity.rs`), with one of them
//! interpreting at a time (the JVM run lock). No interrupt handler and no
//! core-1 task reaches it.
//!
//! [`Core0`] is that argument, made once. The value inside is built from
//! `Cell`s, so every access is safe code and a re-entrant call (an LVGL
//! callback fired from inside a native) reads and writes whole values
//! rather than aliasing a `&mut`.

#![deny(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]

/// A static's worth of state that only core-0 JVM tasks touch.
#[repr(transparent)]
pub struct Core0<T>(T);

// SAFETY: `Core0::new` is `unsafe`, and its contract is that every access
// comes from a core-0 JVM task, which run one at a time. The value is never
// reached from two threads of execution at once.
unsafe impl<T> Sync for Core0<T> {}

impl<T> Core0<T> {
    /// # Safety
    ///
    /// Every access to the value must come from a JVM task pinned to core 0:
    /// never an interrupt handler, never a task on core 1.
    pub const unsafe fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T> core::ops::Deref for Core0<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        &self.0
    }
}
