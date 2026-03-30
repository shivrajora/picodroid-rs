/// Trigger a full chip reset via the RP2040/RP2350 watchdog.
///
/// Both cores reset.  The bootloader re-runs, then `main()` starts fresh,
/// loading the newly-installed PAPK from flash via XIP.
///
/// Uses the watchdog TRIGGER path through the PSM (Power-on State Machine)
/// instead of SYSRESETREQ.  SYSRESETREQ (AIRCR bit 2) is not guaranteed to
/// propagate to a full chip reset on multi-core RP2040/RP2350 — the watchdog
/// path is what the pico-sdk uses and resets all selected subsystems reliably.
///
/// Called from the install task (core 1) after the install is complete.
#[cfg(not(feature = "sim"))]
pub fn flash_trigger_reset() -> ! {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    let p = unsafe { pac::Peripherals::steal() };

    // Tell the PSM to reset every subsystem except the ring and crystal
    // oscillators when the watchdog fires.  WDSEL resets to 0x0000_0000
    // (nothing selected), so we must set it explicitly.
    // Bits: 0=ROSC, 1=XOSC, 2..16=everything else.
    p.PSM.wdsel().write(|w| unsafe { w.bits(0x0001_fffc) });

    // Force-fire the watchdog (CTRL bit 31 = TRIGGER).
    p.WATCHDOG.ctrl().write(|w| unsafe { w.bits(1 << 31) });

    loop {
        cortex_m::asm::nop();
    }
}
