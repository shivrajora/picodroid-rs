// SPDX-License-Identifier: GPL-3.0-only
//! Periodic 16 ms LVGL tick source feeding the unified main queue.
//!
//! Mirrors Android's split between `Looper` (the dispatcher) and
//! `Choreographer` (the vsync-driven frame source): the main loop in
//! `lifecycle::run_activity` is a pure dispatcher that blocks on
//! `main_queue::recv_blocking`, while this module periodically posts
//! `MainTask::LvglTick` so LVGL animations and widget callbacks tick
//! at a steady cadence.
//!
//! Backings:
//! - **Embedded:** a FreeRTOS software timer (auto-reloading, period 16 ms).
//!   The callback runs in the timer service task
//!   (`configTIMER_TASK_PRIORITY = configMAX_PRIORITIES - 1`), so ticks are
//!   punctual. `enqueue_tick` is non-blocking and coalesces, so it's safe
//!   to call from the timer context.
//! - **Sim:** a dedicated `std::thread` with `Instant`-based pacing.
//!
//! `pause()` / `resume()` are used by the lifecycle loop to quiesce the
//! tick source while the display is in low-power sleep — on device this
//! stops the timer entirely, letting the chip enter deeper idle.

const TICK_PERIOD_MS: u32 = 16;

#[cfg(all(not(any(test, feature = "sim")), feature = "family-rp"))]
mod backing {
    use core::cell::UnsafeCell;
    use freertos_rust::{Duration, Timer};

    use super::TICK_PERIOD_MS;

    struct TimerCell(UnsafeCell<Option<Timer>>);
    // SAFETY: the cell is mutated only by the UI thread before the timer
    // is in active use; after that the inner `Timer` is itself thread-safe
    // (operations go through the FreeRTOS timer command queue).
    unsafe impl Sync for TimerCell {}

    static TIMER: TimerCell = TimerCell(UnsafeCell::new(None));

    pub fn start() {
        // SAFETY: callers serialise on the UI thread (see `run_activity`).
        unsafe {
            if let Some(t) = (*TIMER.0.get()).as_ref() {
                let _ = t.start(Duration::ms(0));
                return;
            }
            let timer = Timer::new(Duration::ms(TICK_PERIOD_MS))
                .set_name("lvgl-tick")
                .set_auto_reload(true)
                .create(|_| {
                    crate::system::executors::main_queue::enqueue_tick();
                })
                .expect("lvgl-tick timer alloc");
            timer.start(Duration::ms(0)).expect("lvgl-tick start");
            *TIMER.0.get() = Some(timer);
        }
    }

    #[allow(dead_code)]
    pub fn pause() {
        // SAFETY: see `start`.
        unsafe {
            if let Some(t) = (*TIMER.0.get()).as_ref() {
                let _ = t.stop(Duration::ms(0));
            }
        }
    }

    #[allow(dead_code)]
    pub fn resume() {
        // SAFETY: see `start`.
        unsafe {
            if let Some(t) = (*TIMER.0.get()).as_ref() {
                let _ = t.start(Duration::ms(0));
            }
        }
    }

    pub fn stop() {
        // Stop the timer but keep the `Timer` allocation alive — dropping
        // it would block the caller for up to 1 s waiting on the timer
        // command queue (see `freertos_rust::Timer::drop`). The lvgl-tick
        // is a process-wide singleton; leaking the handle is fine.
        pause();
    }
}

#[cfg(any(test, feature = "sim"))]
mod backing {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::TICK_PERIOD_MS;

    static STARTED: AtomicBool = AtomicBool::new(false);
    static PAUSED: AtomicBool = AtomicBool::new(false);
    static STOPPING: AtomicBool = AtomicBool::new(false);
    static HANDLE: Mutex<Option<thread::JoinHandle<()>>> = Mutex::new(None);

    pub fn start() {
        if STARTED.swap(true, Ordering::SeqCst) {
            // Already running — just unpause.
            PAUSED.store(false, Ordering::SeqCst);
            return;
        }
        STOPPING.store(false, Ordering::SeqCst);
        PAUSED.store(false, Ordering::SeqCst);
        let h = thread::Builder::new()
            .name("lvgl-tick".into())
            .spawn(|| {
                let period = Duration::from_millis(TICK_PERIOD_MS as u64);
                let mut next = Instant::now() + period;
                while !STOPPING.load(Ordering::SeqCst) {
                    let now = Instant::now();
                    if now < next {
                        thread::sleep(next - now);
                    }
                    next = Instant::now() + period;
                    if !PAUSED.load(Ordering::SeqCst) && !STOPPING.load(Ordering::SeqCst) {
                        crate::system::executors::main_queue::enqueue_tick();
                    }
                }
            })
            .expect("spawn lvgl-tick thread");
        *HANDLE.lock().expect("tick-source HANDLE mutex poisoned") = Some(h);
    }

    #[allow(dead_code)]
    pub fn pause() {
        PAUSED.store(true, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn resume() {
        PAUSED.store(false, Ordering::SeqCst);
    }

    pub fn stop() {
        STOPPING.store(true, Ordering::SeqCst);
        if let Some(h) = HANDLE
            .lock()
            .expect("tick-source HANDLE mutex poisoned")
            .take()
        {
            let _ = h.join();
        }
        STARTED.store(false, Ordering::SeqCst);
    }
}

/// Start the periodic 16 ms LVGL tick source. Idempotent; if already
/// running, ensures it is unpaused.
pub fn start() {
    backing::start()
}

/// Stop posting ticks but keep the source ready to resume. Used by the
/// activity loop's low-power sleep branch (only reachable on boards with
/// physical buttons), so this is dead on sim / touch-only builds.
#[allow(dead_code)]
pub fn pause() {
    backing::pause()
}

/// Resume posting ticks after a [`pause`] call. See [`pause`] for why
/// this is `#[allow(dead_code)]`.
#[allow(dead_code)]
pub fn resume() {
    backing::resume()
}

/// Tear down the tick source. On embedded the timer is left allocated
/// (singleton); on sim the thread is signalled and joined.
pub fn stop() {
    backing::stop()
}
