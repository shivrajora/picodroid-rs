// SPDX-License-Identifier: GPL-3.0-only
/// Block the calling task for `ms`.
///
/// Deliberately unconditional. This used to short-circuit on a pending
/// debug-bridge stop request, which made it disagree with the simulator and
/// made the behaviour an unwritten obligation on every other family. That
/// check now lives in shared code, at the `SystemClock.sleep` native — see
/// `picodroid_core::os::system_clock::sleep`.
pub fn sleep(ms: u32) {
    freertos_rust::CurrentTask::delay(freertos_rust::Duration::ms(ms));
}

pub fn elapsed_realtime_nanos() -> i64 {
    // Use the RAW timer registers with the pico-sdk `time_us_64` hi-lo-hi
    // loop. The latched TIMEHR/TIMELR pair is a single shared hardware
    // latch — concurrent readers (JVM task + sensor sampler + Java
    // threads, or the other core) interleave their latch cycles and
    // corrupt the high word, producing 2^32 µs (~71.6 min) jumps. The raw
    // loop is lock-free and safe for any number of readers: retry until
    // the high word is stable across the low-word read.
    #[cfg(feature = "chip-rp2040")]
    {
        use rp_pico::hal::pac;
        // SAFETY: read-only register access, no side effects.
        let p = unsafe { pac::Peripherals::steal() };
        let mut hi = p.TIMER.timerawh().read().bits();
        let us = loop {
            let lo = p.TIMER.timerawl().read().bits();
            let next_hi = p.TIMER.timerawh().read().bits();
            if hi == next_hi {
                break ((hi as u64) << 32) | (lo as u64);
            }
            hi = next_hi;
        };
        (us * 1000) as i64
    }
    #[cfg(feature = "chip-rp2350")]
    {
        use rp235x_hal::pac;
        // SAFETY: read-only register access, no side effects.
        let p = unsafe { pac::Peripherals::steal() };
        let mut hi = p.TIMER0.timerawh().read().bits();
        let us = loop {
            let lo = p.TIMER0.timerawl().read().bits();
            let next_hi = p.TIMER0.timerawh().read().bits();
            if hi == next_hi {
                break ((hi as u64) << 32) | (lo as u64);
            }
            hi = next_hi;
        };
        (us * 1000) as i64
    }
}
