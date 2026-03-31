pub fn set_direction(pin: u8, direction: i32) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    // SAFETY: We are the OS — we own all hardware.
    let p = unsafe { pac::Peripherals::steal() };

    // IO_BANK0 and PADS_BANK0 are held in reset after boot unless explicitly released.
    // Unreset them now (idempotent — safe to call even if already done).
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}

    // Set GPIO function to SIO (function select 5)
    p.IO_BANK0
        .gpio(pin as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(5) });

    // Configure pad: disable input buffer, not open-drain.
    // On RP2350 the pad defaults to ISO=1 (electrically isolated); clear it.
    p.PADS_BANK0.gpio(pin as usize).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        w.ie().clear_bit().od().clear_bit()
    });

    // Enable output driver for this pin
    p.SIO
        .gpio_oe_set()
        .write(|w| unsafe { w.bits(1u32 << pin) });

    // Set initial output level based on direction constant
    // DIRECTION_OUT_INITIALLY_HIGH = 1, DIRECTION_OUT_INITIALLY_LOW = 2
    if direction == 1 {
        p.SIO
            .gpio_out_set()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    } else {
        p.SIO
            .gpio_out_clr()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    }
}

pub fn set_value(pin: u8, high: bool) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    if high {
        p.SIO
            .gpio_out_set()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    } else {
        p.SIO
            .gpio_out_clr()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    }
}
