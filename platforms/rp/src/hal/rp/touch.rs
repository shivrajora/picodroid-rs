// SPDX-License-Identifier: GPL-3.0-only
//! Touch facade — delegates to the controller `[touch] driver` names.
//!
//! Two are wired up, and they have almost nothing in common below this file:
//! the XPT2046 is a resistive ADC sharing the display's SPI bus and needing
//! calibration bounds, the GT911 a capacitive controller on its own I2C bus
//! that reports finished pixels. What they share is everything above — the
//! scripted-touch override, the read path and the `HalTouch` surface — so only
//! the type alias and the constructor are per-driver.
//!
//! On boards without a `[touch]` section in board.toml (`has_touch` cfg absent),
//! all functions are no-ops / return `None`.

#[cfg(has_touch)]
mod inner {
    use crate::hal::input_pin::RpInputPin;
    use crate::hal::output_pin::RpOutputPin;
    #[cfg(touch_xpt2046)]
    use crate::hal::spi_bus::RpSpiBus;
    use core::ptr::addr_of_mut;
    use picodroid_core::hal::touch_override::{OverrideSample, TouchOverride};

    // Scripted touch (PDB `CMD_INPUT` tap/swipe): while engaged, `read_point`
    // reports the scripted point instead of the panel, so an injected tap or
    // swipe runs the exact same `touch_read_cb` → LVGL hit-test / gesture →
    // Java `MotionEvent` pipeline as a real finger. The state machine is
    // core's; the PDB task drives it from its own task.
    static OVERRIDE: TouchOverride = TouchOverride::new();

    pub fn inject_override(x: u16, y: u16) {
        OVERRIDE.inject(x, y);
        // A scripted touch moves no pin: wake the sampler ourselves.
        crate::hal::gpio::kick_touch_irq();
    }
    pub fn release_override() {
        OVERRIDE.release();
        crate::hal::gpio::kick_touch_irq();
    }
    /// The sampler's wait: the panel's INT line where it is armed (the
    /// GT911's, after its reset), a plain sleep otherwise.
    pub fn wait_irq(timeout_ms: u32) -> bool {
        crate::hal::gpio::wait_touch_irq(timeout_ms)
    }
    pub fn clear_override() {
        OVERRIDE.clear()
    }

    // board.toml lists every pin/geometry field the panel has; a given build
    // consumes only the subset its code path touches. Scoped here rather than
    // over the whole HAL module so real rot outside the generated table stays
    // visible.
    #[allow(dead_code)]
    mod generated {
        include!(concat!(env!("OUT_DIR"), "/touch_config.rs"));
    }
    // board.toml lists every pin/geometry field the panel has; a given build
    // consumes only the subset its code path touches. Scoped here rather than
    // over the whole HAL module so real rot outside the generated table stays
    // visible.
    #[allow(dead_code)]
    mod display_generated {
        include!(concat!(env!("OUT_DIR"), "/display_config.rs"));
    }

    // ── XPT2046: resistive, on the display's SPI bus ────────────────────────

    #[cfg(touch_xpt2046)]
    type Touch = crate::drivers::xpt2046::Xpt2046<RpSpiBus, RpOutputPin>;

    /// The XPT2046's MISO is a second input on the display's SPI bus, so the
    /// pad has to be handed to the SPI peripheral by hand — `RpSpiBus::handle`
    /// attaches to an already-running bus and configures no pins.
    #[cfg(touch_xpt2046)]
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

    #[cfg(touch_xpt2046)]
    fn build_touch() -> Touch {
        configure_touch_miso();
        let _irq = RpInputPin::new(generated::TOUCH_PIN_IRQ, true);

        let spi = RpSpiBus::handle(display_generated::SPI_ID);
        let cs = RpOutputPin::new(generated::TOUCH_PIN_CS, true);

        let mut touch = Touch::new(
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
        touch
    }

    // ── GT911: capacitive, on its own I2C bus ───────────────────────────────

    /// Adapter from the driver's tiny bus trait to this family's I2C, the same
    /// shape the sensor sampler uses for the BME688 and LTR559.
    #[cfg(touch_gt911)]
    struct RpI2cBus {
        bus_id: u8,
    }

    #[cfg(touch_gt911)]
    impl crate::drivers::gt911::I2cBus for RpI2cBus {
        fn write(&mut self, addr: u8, data: &[u8]) -> i32 {
            crate::hal::i2c::write_slice(self.bus_id, addr, data)
        }
        fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32 {
            crate::hal::i2c::read_slice(self.bus_id, addr, buf)
        }
    }

    #[cfg(touch_gt911)]
    type Touch = crate::drivers::gt911::Gt911<RpI2cBus>;

    #[cfg(touch_gt911)]
    fn build_touch() -> Touch {
        use crate::drivers::gt911::Gt911;
        use crate::hal::delay::RpDelay;

        // The reset picks the bus address, so it has to happen before the bus
        // carries any traffic. INT is an output only for the duration of that
        // pulse; the controller drives it as an interrupt line afterwards, so
        // it is flipped to an input the moment the reset returns.
        let mut int = RpOutputPin::new(generated::TOUCH_PIN_INT, false);
        let mut rst = RpOutputPin::new(generated::TOUCH_PIN_RST, false);
        let mut delay = RpDelay::new();
        // `Touch`, not `Gt911`: `reset` names no `I2cBus` in its signature, so
        // the bus type has to come from the alias.
        Touch::reset(&mut int, &mut rst, &mut delay, generated::TOUCH_ADDR);
        let _int_in = RpInputPin::new(generated::TOUCH_PIN_INT, true);
        // From here the controller drives INT; both edges wake the sampler,
        // so it reads the panel when the panel has something to say instead
        // of a hundred times a second regardless.
        crate::hal::gpio::arm_touch_irq(generated::TOUCH_PIN_INT);

        crate::hal::i2c::init_with_pins(
            generated::TOUCH_I2C_ID,
            generated::TOUCH_I2C_SDA,
            generated::TOUCH_I2C_SCL,
        );

        let bus = RpI2cBus {
            bus_id: generated::TOUCH_I2C_ID,
        };
        let mut touch = Gt911::new(
            bus,
            generated::TOUCH_ADDR,
            display_generated::SCREEN_WIDTH,
            display_generated::SCREEN_HEIGHT,
            generated::TOUCH_SWAP_XY,
        );
        // Log what the controller says it is, either way. This is the cheapest
        // positive identification of the carrier there is — no other board
        // picodroid targets has a GT911 — so a bring-up can tell "the right
        // board, wired correctly" from "nothing on this bus" with no
        // instrumentation beyond the boot log. The panel size is the
        // controller's own configuration, so it also catches a panel that
        // disagrees with board.toml.
        //
        // A failure is not fatal: the driver stays live and every read returns
        // `None`, which degrades to "nobody is touching the screen" rather
        // than taking the boot down on a board whose panel is unplugged.
        match touch.init() {
            Ok(id) => picodroid_core::pd_info!(
                "[touch] GT911 at {=u8:#04x}: fw={=u16:#06x} vendor={=u8:#04x} panel={=u16}x{=u16}",
                generated::TOUCH_ADDR,
                id.firmware,
                id.vendor,
                id.x_resolution,
                id.y_resolution,
            ),
            Err(e) => picodroid_core::pd_warn!(
                "[touch] GT911 did not answer at {=u8:#04x}: {:?}",
                generated::TOUCH_ADDR,
                defmt::Debug2Format(&e)
            ),
        }
        touch
    }

    // ── Shared facade ───────────────────────────────────────────────────────

    static mut TOUCH: Option<Touch> = None;

    pub fn init() {
        let touch = build_touch();
        unsafe {
            addr_of_mut!(TOUCH).write(Some(touch));
        }
    }

    fn touch() -> &'static mut Touch {
        unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap() }
    }

    pub fn read_point() -> Option<(u16, u16)> {
        match OVERRIDE.sample() {
            OverrideSample::Inactive => touch().read_point(),
            OverrideSample::Pressed(x, y) => Some((x, y)),
            OverrideSample::Lifted(..) => None,
        }
    }

    pub fn read_raw_unfiltered() -> (u16, u16) {
        touch().read_raw_unfiltered()
    }

    #[cfg(touch_xpt2046)]
    pub fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16) {
        touch().set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max);
    }

    /// No-op on a capacitive panel: the GT911 reports finished pixel
    /// coordinates from its own configuration, so there is nothing on this side
    /// to calibrate. The seam keeps the method because `HalTouch` is one trait
    /// for every controller.
    #[cfg(touch_gt911)]
    pub fn set_calibration(_: u16, _: u16, _: u16, _: u16) {}
}

#[cfg(not(has_touch))]
mod inner {
    pub fn init() {}
    pub fn read_point() -> Option<(u16, u16)> {
        None
    }
    pub fn read_raw_unfiltered() -> (u16, u16) {
        (0, 0)
    }
    pub fn set_calibration(_: u16, _: u16, _: u16, _: u16) {}
    // No panel to drive — scripted-touch injection is a no-op (the PDB
    // `CMD_INPUT` handler reports STATUS_ERR for tap/swipe on such boards).
    pub fn inject_override(_: u16, _: u16) {}
    pub fn release_override() {}
    pub fn clear_override() {}
    pub fn wait_irq(timeout_ms: u32) -> bool {
        picodroid_core::rtos::delay_ms(timeout_ms);
        false
    }
}

pub use inner::*;
