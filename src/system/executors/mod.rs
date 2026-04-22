//! Java executors: main-thread FIFO + background thread pool.
//!
//! `main_queue` owns the unified FIFO that interleaves LVGL ticks with
//! user-submitted `Runnable`s on the UI thread. `background_pool` owns the
//! pre-spawned FreeRTOS worker tasks that drain a shared work queue.

#[cfg(not(test))]
pub mod background_pool;
pub mod main_queue;
