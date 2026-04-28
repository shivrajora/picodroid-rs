pub fn sleep(ms: u32) {
    if crate::pdb::pending::is_stop_jvm() {
        return;
    }
    freertos_rust::CurrentTask::delay(freertos_rust::Duration::ms(ms));
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
