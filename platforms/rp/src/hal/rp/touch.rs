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
}

pub use inner::*;
