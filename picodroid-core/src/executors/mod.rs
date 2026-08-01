// SPDX-License-Identifier: GPL-3.0-only
//! Java executors: main-thread FIFO + background worker pool.
//!
//! [`main_queue`] owns the unified FIFO that interleaves UI ticks with
//! user-submitted `Runnable`s on the UI thread — the `Looper` analogue.
//! [`tick_source`] is the `Choreographer` analogue that paces it.
//! [`background_pool`] owns the worker tasks that drain a shared work queue
//! off the UI thread. [`serial_worker`] is the odd one out — not a Java
//! executor but the same machinery, a single task that runs submitted
//! closures one at a time so its callers cannot interleave (`crate::fs`).
//!
//! All three used to carry paired FreeRTOS and `std` backings inline. Those
//! now live behind [`crate::rtos`], so these modules compile once for every
//! target — including plain host tests, which is why none of them is
//! `cfg`-gated any more.

pub mod background_pool;
pub mod main_queue;
pub mod serial_worker;
pub mod tick_source;
