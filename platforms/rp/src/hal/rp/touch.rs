// SPDX-License-Identifier: GPL-3.0-only
//! Touch facade — delegates to the generic XPT2046 driver via board config.
//!
//! On boards without a `[touch]` section in board.toml (`has_touch` cfg absent),
//! all functions are no-ops / return `None`.

#[cfg(has_touch)]
mod inner {
    use crate::drivers::xpt2046::Xpt2046;
    use crate::hal::input_pin::RpInputPin;
    use crate::hal::output_pin::RpOutputPin;
    use crate::hal::spi_bus::RpSpiBus;
    use core::ptr::addr_of_mut;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    // ── Scripted-touch override (PDB `CMD_INPUT` tap/swipe) ──────────────────
    //
    // While active, `read_point()` returns these coordinates instead of the
    // real panel sample, so an injected tap/swipe runs the exact same
    // `touch_read_cb` → LVGL hit-test / gesture → Java `MotionEvent` pipeline
    // as a real finger. Mirrors the sim's `TOUCH_OVERRIDE_*` mechanism in
    // `hal::sim::display`. Atomics: the PDB task drives these from its own core.
    static OVERRIDE_ACTIVE: AtomicBool = AtomicBool::new(false);
    static OVERRIDE_PRESSED: AtomicBool = AtomicBool::new(false);
    /// Packed `(x << 16) | y` so a move is a single atomic store.
    static OVERRIDE_POS: AtomicU32 = AtomicU32::new(0);

    /// Begin/continue a scripted touch at `(x, y)` (press or drag-move).
    pub fn inject_override(x: u16, y: u16) {
        OVERRIDE_POS.store(((x as u32) << 16) | y as u32, Ordering::Relaxed);
        OVERRIDE_PRESSED.store(true, Ordering::Relaxed);
        OVERRIDE_ACTIVE.store(true, Ordering::Relaxed);
    }

    /// Lift the scripted touch but keep the override engaged, so the RELEASE
    /// edge is observed from the scripted position before real sampling resumes.
    pub fn release_override() {
        OVERRIDE_PRESSED.store(false, Ordering::Relaxed);
    }

    /// Disengage the override entirely; `read_point()` resumes real sampling.
    pub fn clear_override() {
        OVERRIDE_PRESSED.store(false, Ordering::Relaxed);
        OVERRIDE_ACTIVE.store(false, Ordering::Relaxed);
    }

    mod generated {
        include!(concat!(env!("OUT_DIR"), "/touch_config.rs"));
    }
    mod display_generated {
        include!(concat!(env!("OUT_DIR"), "/display_config.rs"));
    }

    type Touch = Xpt2046<RpSpiBus, RpOutputPin>;

    static mut TOUCH: Option<Touch> = None;

    fn configure_touch_miso() {
        #[cfg(feature = "chip-rp2350")]
        use rp235x_hal::pac;
        #[cfg(feature = "chip-rp2040")]
        use rp_pico::hal::pac;
        let p = unsafe { pac::Peripherals::steal() };

        p.IO_BANK0
            .gpio(generated::TOUCH_PIN_MISO as usize)
            .gpio_ctrl()
            .write(|w| unsafe { w.funcsel().bits(1) }); // 1 = SPI
        p.PADS_BANK0
            .gpio(generated::TOUCH_PIN_MISO as usize)
            .write(|w| {
                #[cfg(feature = "chip-rp2350")]
                let w = w.iso().clear_bit();
                w.ie().set_bit().od().clear_bit()
            });
    }

    pub fn init() {
        configure_touch_miso();
        let _irq = RpInputPin::new(generated::TOUCH_PIN_IRQ, true);

        let spi = RpSpiBus::handle(display_generated::SPI_ID);
        let cs = RpOutputPin::new(generated::TOUCH_PIN_CS, true);

        let mut touch = Xpt2046::new(
            spi,
            cs,
            generated::TOUCH_SPI_FREQ,
            display_generated::SPI_FREQ,
            display_generated::SCREEN_WIDTH,
            display_generated::SCREEN_HEIGHT,
            generated::TOUCH_CAL_X_MIN,
            generated::TOUCH_CAL_X_MAX,
            generated::TOUCH_CAL_Y_MIN,
            generated::TOUCH_CAL_Y_MAX,
        );
        touch.set_swap_xy(generated::TOUCH_SWAP_XY);
        touch.init();

        unsafe {
            addr_of_mut!(TOUCH).write(Some(touch));
        }
    }

    fn touch() -> &'static mut Touch {
        unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap() }
    }

    pub fn read_point() -> Option<(u16, u16)> {
        if OVERRIDE_ACTIVE.load(Ordering::Relaxed) {
            if OVERRIDE_PRESSED.load(Ordering::Relaxed) {
                let p = OVERRIDE_POS.load(Ordering::Relaxed);
                return Some(((p >> 16) as u16, (p & 0xFFFF) as u16));
            }
            return None;
        }
        touch().read_point()
    }

    pub fn read_raw() -> Option<(u16, u16)> {
        touch().read_raw()
    }

    pub fn read_raw_unfiltered() -> (u16, u16) {
        touch().read_raw_unfiltered()
    }

    pub fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16) {
        touch().set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max);
    }

    pub fn set_rejection_range(lo: u16, hi: u16) {
        touch().set_rejection_range(lo, hi);
    }
}

#[cfg(not(has_touch))]
mod inner {
    pub fn init() {}
    pub fn read_point() -> Option<(u16, u16)> {
        None
    }
    pub fn read_raw() -> Option<(u16, u16)> {
        None
    }
    pub fn read_raw_unfiltered() -> (u16, u16) {
        (0, 0)
    }
    pub fn set_calibration(_: u16, _: u16, _: u16, _: u16) {}
    pub fn set_rejection_range(_: u16, _: u16) {}
    // No panel to drive — scripted-touch injection is a no-op (the PDB
    // `CMD_INPUT` handler reports STATUS_ERR for tap/swipe on such boards).
    pub fn inject_override(_: u16, _: u16) {}
    pub fn release_override() {}
    pub fn clear_override() {}
}

pub use inner::*;
