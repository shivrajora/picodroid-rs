pub fn sleep(ms: u32) {
    // Fast-path: if the JVM is being stopped (pdb install), skip the
    // delay entirely so the interpreter reaches its interrupt check
    // within microseconds instead of blocking for the full duration.
    // request_stop_and_park() also calls abort_jvm_delay() to wake us
    // from any in-progress vTaskDelay, so we only need this guard to
    // prevent re-entering a new delay after being aborted.
    if crate::pdb::pending::STOP_JVM.load(core::sync::atomic::Ordering::Relaxed) {
        return;
    }
    freertos_rust::CurrentTask::delay(freertos_rust::Duration::ms(ms));
}

/// A jitter-free frame pacer backed by `vTaskDelayUntil`.
///
/// Calling `pace(16)` sleeps for exactly `16 ms` minus the time already
/// consumed since the last wakeup, keeping a steady frame rate regardless
/// of how long tick + dispatch takes each iteration.
pub struct FramePacer {
    inner: freertos_rust::TaskDelay,
}

impl FramePacer {
    pub fn new() -> Self {
        Self {
            inner: freertos_rust::TaskDelay::new(),
        }
    }

    /// Sleep until the next frame boundary (`period_ms` after last wakeup).
    pub fn pace(&mut self, period_ms: u32) {
        self.inner
            .delay_until(freertos_rust::Duration::ms(period_ms));
    }
}

pub fn elapsed_realtime_nanos() -> i64 {
    #[cfg(feature = "chip-rp2040")]
    {
        use rp_pico::hal::pac;
        // SAFETY: read-only register access, no side effects.
        let p = unsafe { pac::Peripherals::steal() };
        // Reading TIMEHR latches TIMELR for a consistent 64-bit read.
        let hi = p.TIMER.timehr().read().bits();
        let lo = p.TIMER.timelr().read().bits();
        let us = ((hi as u64) << 32) | (lo as u64);
        (us * 1000) as i64
    }
    #[cfg(feature = "chip-rp2350-hal")]
    {
        use rp235x_hal::pac;
        // SAFETY: read-only register access, no side effects.
        let p = unsafe { pac::Peripherals::steal() };
        // Reading TIMEHR latches TIMELR for a consistent 64-bit read.
        let hi = p.TIMER0.timehr().read().bits();
        let lo = p.TIMER0.timelr().read().bits();
        let us = ((hi as u64) << 32) | (lo as u64);
        (us * 1000) as i64
    }
}
