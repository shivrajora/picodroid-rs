// SPDX-License-Identifier: GPL-3.0-only
//! Which task owns the UI, and a one-shot warning for anyone else who
//! touches it.
//!
//! Android throws `CalledFromWrongThreadException` when a View is touched
//! off the thread that created its hierarchy. Here the widget tree and
//! LVGL's own state are unguarded globals, so the same mistake is not an
//! exception but memory corruption in C — and a hard fault on device hides
//! the cause. The UI task records itself on entry to the activity loop;
//! every View native dispatched from another task logs once,
//! naming the fix. A warning rather than an error: a checkThread in every
//! View native would cost flash on every board, and the log line already
//! points at the exact idiom to use.
//!
//! Its own module rather than a corner of `graphics::lvgl` so the unit test
//! runs: that module is compiled out under `cargo test`.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The task that owns the UI — `0` until [`note_ui_task`] runs.
static UI_TASK: AtomicUsize = AtomicUsize::new(0);
/// Plain load/store rather than `swap`: thumbv6m has no atomic RMW, and a
/// duplicated warning under a race is harmless.
static OFF_THREAD_WARNED: AtomicBool = AtomicBool::new(false);

/// Record the calling task as the UI task. Called by the activity loop on
/// entry; re-arms the one-shot warning for a reloaded app.
pub fn note_ui_task() {
    UI_TASK.store(crate::rtos::task_current(), Ordering::Relaxed);
    OFF_THREAD_WARNED.store(false, Ordering::Relaxed);
}

/// Whether the calling task is the UI task. `true` before any UI task has
/// been recorded (boot-time drawing, unit tests).
pub fn is_ui_task() -> bool {
    let ui = UI_TASK.load(Ordering::Relaxed);
    ui == 0 || crate::rtos::task_current() == ui
}

/// Warn — once per app run — if the caller is not the UI task. Returns
/// whether a warning was emitted, for tests.
pub fn warn_if_off_ui_thread() -> bool {
    if is_ui_task() || OFF_THREAD_WARNED.load(Ordering::Relaxed) {
        return false;
    }
    OFF_THREAD_WARNED.store(true, Ordering::Relaxed);
    crate::pd_warn!(
        "View touched off the UI thread (Android: CalledFromWrongThreadException) — \
         post it through Executors.mainExecutor().execute(Runnable)"
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // Process-global state; serialise.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        let g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        UI_TASK.store(0, Ordering::Relaxed);
        OFF_THREAD_WARNED.store(false, Ordering::Relaxed);
        g
    }

    #[test]
    fn nobody_is_off_thread_before_a_ui_task_is_recorded() {
        let _g = serial();
        assert!(is_ui_task());
        assert!(!warn_if_off_ui_thread());
        assert!(std::thread::spawn(is_ui_task).join().unwrap());
    }

    #[test]
    fn only_the_recording_task_is_the_ui_task() {
        // Under `cargo test` a "task" is a host thread (std backing).
        let _g = serial();
        note_ui_task();
        assert!(is_ui_task());
        assert!(!warn_if_off_ui_thread());
        let other = std::thread::spawn(is_ui_task).join().unwrap();
        assert!(!other, "another thread is not the UI task");
    }

    #[test]
    fn an_off_thread_touch_warns_exactly_once_per_app_run() {
        let _g = serial();
        note_ui_task();
        let warned: Vec<bool> =
            std::thread::spawn(|| vec![warn_if_off_ui_thread(), warn_if_off_ui_thread()])
                .join()
                .unwrap();
        assert_eq!(warned, vec![true, false]);
        // A reload re-arms it.
        note_ui_task();
        assert!(std::thread::spawn(warn_if_off_ui_thread).join().unwrap());
    }
}
