// SPDX-License-Identifier: GPL-3.0-only
//! `embedded-hal` `DelayNs` for the RP family: the kernel for anything a
//! millisecond or longer, the cycle counter for the rest.
//!
//! The panel and touch drivers take a `DelayNs` for their datasheet
//! settling waits — ST7789 ~350 ms and ST7796 ~540 ms of them at init,
//! 120 ms on every `sleep_out`, 76 ms in the GT911 reset — and run it from
//! the JVM task after the scheduler has started. A cycle-count delay there
//! held the UI task's core for the whole wait, every other task at the JVM
//! tier, the background pool and the sensor sampler included
//! (docs/scheduling-audit-2026-09.md, F2). So `delay_ms` and `delay_us`
//! sleep on the kernel whenever it is running, rounded up one tick so the
//! wait is at least what the datasheet asked, and burn cycles only for a
//! sub-tick remainder or before the scheduler exists (a driver reset issued
//! from boot code). `delay_ns` is always a spin: nothing needs a nanosecond
//! wait the kernel could serve.

use embedded_hal::delay::DelayNs;

/// The RP family's delay: kernel-backed at millisecond scale, a cycle count
/// below it.
pub struct RpDelay;

impl RpDelay {
    pub fn new() -> Self {
        Self
    }
}

// Cycles per nanosecond at 150 MHz ≈ 0.15, so ns / 7 ≈ cycles.
// At 125 MHz (RP2040) this is slightly conservative (slower delays), which is safe.
#[cfg(feature = "chip-rp2350")]
const NS_PER_CYCLE: u32 = 7; // 150 MHz: ~6.67 ns/cycle, rounded to 7
#[cfg(feature = "chip-rp2040")]
const NS_PER_CYCLE: u32 = 8; // 125 MHz: 8 ns/cycle

fn scheduler_running() -> bool {
    freertos_rust::FreeRtosUtils::scheduler_state()
        == freertos_rust::FreeRtosSchedulerState::Running
}

// `inline(never)` on all three: the panel drivers call these from a dozen
// sites each, and an inlined copy of the scheduler check plus the kernel
// call at every one cost ~600 B of RP2040 flash. One body each is a call.
impl DelayNs for RpDelay {
    #[inline(never)]
    fn delay_ns(&mut self, ns: u32) {
        // spin-ok: a sub-tick remainder, or a wait issued before the scheduler runs
        cortex_m::asm::delay(ns / NS_PER_CYCLE);
    }

    #[inline(never)]
    fn delay_us(&mut self, us: u32) {
        if us >= 1000 && scheduler_running() {
            self.delay_ms(us / 1000);
            self.delay_ns((us % 1000) * 1000);
        } else {
            self.delay_ns(us.saturating_mul(1000));
        }
    }

    #[inline(never)]
    fn delay_ms(&mut self, ms: u32) {
        if ms > 0 && scheduler_running() {
            // One tick more than asked: vTaskDelay(n) may return after n - 1
            // whole ticks plus a fraction, and these are datasheet minimums.
            freertos_rust::CurrentTask::delay(freertos_rust::Duration::ms(ms + 1));
        } else {
            for _ in 0..ms {
                self.delay_ns(1_000_000);
            }
        }
    }
}
