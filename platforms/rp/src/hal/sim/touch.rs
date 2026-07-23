// SPDX-License-Identifier: GPL-3.0-only
//! Simulator touch backend — feeds minifb mouse position through the same
//! `Xpt2046` driver code that runs on hardware so calibration, swap_xy,
//! median sampling, and rejection logic all exercise identically in sim.
//!
//! `FakeXptSpi` is the trick: it implements `embedded_hal::spi::SpiBus`
//! and synthesises 12-bit ADC values from current mouse position by
//! inverting `Xpt2046::map_range`. The driver's forward mapping then
//! round-trips back to (within ±1 px due to integer truncation) the
//! original mouse pixel — but the entire driver pipeline ran on the way.
//!
//! Env vars:
//! - `PICODROID_SIM_PERFECT_TOUCH=1` — disable ±2 LSB jitter (default: on)
//!
//! See `/home/shiv/.claude/plans/lovely-roaming-parasol.md` for context.
#![allow(dead_code)]

#[cfg(has_touch)]
mod inner {
    use super::super::output_pin::SimOutputPin;
    use crate::drivers::xpt2046::Xpt2046;
    use crate::drivers::SpiFreqSwitch;
    use core::convert::Infallible;
    use core::ptr::addr_of_mut;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use embedded_hal::spi::{ErrorType, SpiBus};

    mod generated {
        include!(concat!(env!("OUT_DIR"), "/touch_config.rs"));
    }
    mod display_generated {
        include!(concat!(env!("OUT_DIR"), "/display_config.rs"));
    }

    // XPT2046 control bytes — must match the driver.
    const CMD_READ_X: u8 = 0xD0;
    const CMD_READ_Y: u8 = 0x90;

    /// `1` when the user has set `PICODROID_SIM_PERFECT_TOUCH=1` — turns off
    /// the ±2 LSB jitter so round-trip is bit-exact (modulo the inherent
    /// ±1 truncation in `map_range`). Default: jitter on.
    static PERFECT: AtomicBool = AtomicBool::new(false);
    /// xorshift32 state for jitter — keyed off frame count, not mouse pos,
    /// so a stationary tap exercises the median filter as on hardware.
    static RNG: AtomicU32 = AtomicU32::new(0x1234_5678);

    fn next_jitter() -> i32 {
        let mut x = RNG.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        RNG.store(x, Ordering::Relaxed);
        // Map to -2..=2 (5 buckets — wider would push out of median).
        ((x % 5) as i32) - 2
    }

    /// Fake SpiBus that synthesises 12-bit ADC samples from current mouse
    /// position. The driver's protocol is one 3-byte transfer per axis: the
    /// command byte goes out as `tx[0]`, and the 12-bit ADC reading comes
    /// back packed as `((rx[1] << 4) | (rx[2] >> 4))`.
    pub struct FakeXptSpi;

    impl FakeXptSpi {
        pub const fn new() -> Self {
            Self
        }

        /// Inverse of `Xpt2046::map_range` for `out_min=0, out_max=screen-1`.
        /// Returns a raw ADC value such that the forward map lands back on
        /// `s` (within ±1 due to integer truncation in the driver).
        ///
        /// Made `pub(super)` for the round-trip unit test in `inner::tests`.
        pub(super) fn screen_to_raw(s: u16, cal_min: u16, cal_max: u16, screen: u16) -> u16 {
            if screen <= 1 {
                return cal_min;
            }
            let s = s.min(screen - 1) as i32;
            let num = s * (cal_max as i32 - cal_min as i32);
            let den = (screen as i32) - 1;
            let val = cal_min as i32 + num / den;
            let (lo, hi) = if cal_min <= cal_max {
                (cal_min as i32, cal_max as i32)
            } else {
                (cal_max as i32, cal_min as i32)
            };
            val.clamp(lo, hi) as u16
        }

        fn synth(cmd: u8) -> u16 {
            let (pressed, mx, my) = super::super::display::mouse_state();
            if !pressed {
                // Force value outside default reject range so driver returns None.
                return 0;
            }
            // Driver swap semantics:
            //   if swap_xy { return (raw_y, raw_x) } else { return (raw_x, raw_y) }
            // The returned tuple's first element is then mapped via cal_x → screen X.
            // So when swap_xy=true, CMD_READ_Y output is what becomes screen X
            // (and CMD_READ_X output becomes screen Y).
            let swap = generated::TOUCH_SWAP_XY;
            let w = display_generated::SCREEN_WIDTH;
            let h = display_generated::SCREEN_HEIGHT;
            let raw = match (cmd, swap) {
                (CMD_READ_X, false) | (CMD_READ_Y, true) => Self::screen_to_raw(
                    mx,
                    generated::TOUCH_CAL_X_MIN,
                    generated::TOUCH_CAL_X_MAX,
                    w,
                ),
                (CMD_READ_Y, false) | (CMD_READ_X, true) => Self::screen_to_raw(
                    my,
                    generated::TOUCH_CAL_Y_MIN,
                    generated::TOUCH_CAL_Y_MAX,
                    h,
                ),
                _ => 0,
            };

            if PERFECT.load(Ordering::Relaxed) {
                raw
            } else {
                let jittered = (raw as i32 + next_jitter()).clamp(0, 4095);
                jittered as u16
            }
        }
    }

    impl SpiFreqSwitch for FakeXptSpi {
        fn set_frequency(&mut self, _freq_hz: u32) {}
    }

    impl ErrorType for FakeXptSpi {
        type Error = Infallible;
    }

    impl SpiBus<u8> for FakeXptSpi {
        fn read(&mut self, words: &mut [u8]) -> Result<(), Infallible> {
            words.fill(0);
            Ok(())
        }
        fn write(&mut self, _words: &[u8]) -> Result<(), Infallible> {
            Ok(())
        }
        fn transfer(&mut self, rx: &mut [u8], tx: &[u8]) -> Result<(), Infallible> {
            rx.fill(0);
            // Driver always sends [cmd, 0, 0]; result is ((rx[1] << 4) | (rx[2] >> 4)).
            if !tx.is_empty() && rx.len() >= 3 && (tx[0] == CMD_READ_X || tx[0] == CMD_READ_Y) {
                let raw = Self::synth(tx[0]) & 0x0FFF;
                rx[1] = (raw >> 4) as u8;
                rx[2] = ((raw & 0x0F) << 4) as u8;
            }
            Ok(())
        }
        fn transfer_in_place(&mut self, _words: &mut [u8]) -> Result<(), Infallible> {
            Ok(())
        }
        fn flush(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
    }

    type Touch = Xpt2046<FakeXptSpi, SimOutputPin>;
    static mut TOUCH: Option<Touch> = None;

    pub fn init() {
        if std::env::var("PICODROID_SIM_PERFECT_TOUCH")
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            PERFECT.store(true, Ordering::Relaxed);
        }

        let cs = SimOutputPin::new(generated::TOUCH_PIN_CS, true);
        let mut touch = Xpt2046::new(
            FakeXptSpi::new(),
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
        let mode = if PERFECT.load(Ordering::Relaxed) {
            "perfect"
        } else {
            "jittered"
        };
        println!(
            "[sim] Touch: XPT2046 driver active ({mode}, swap_xy={}, cal_x={}..{}, cal_y={}..{})",
            generated::TOUCH_SWAP_XY,
            generated::TOUCH_CAL_X_MIN,
            generated::TOUCH_CAL_X_MAX,
            generated::TOUCH_CAL_Y_MIN,
            generated::TOUCH_CAL_Y_MAX,
        );
    }

    fn touch() -> &'static mut Touch {
        unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap() }
    }

    pub fn read_point() -> Option<(u16, u16)> {
        touch().read_point()
    }

    // Scripted-touch override (HAL contract parity with hardware). Delegates to
    // the display's `TOUCH_OVERRIDE_*` machinery, which feeds `mouse_state()` →
    // the full `FakeXptSpi` → `Xpt2046` pipeline. The device PDB `CMD_INPUT`
    // handler is host-only code, so on sim these are exercised only if called
    // directly, but they keep the sim/hardware HAL surface identical.
    pub fn inject_override(x: u16, y: u16) {
        super::super::display::set_touch_override(x, y);
    }
    pub fn release_override() {
        super::super::display::touch_override_release();
    }
    pub fn clear_override() {
        super::super::display::clear_touch_override();
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

    #[cfg(test)]
    mod tests {
        use super::FakeXptSpi;

        /// Replicate the driver's `map_range` so we can verify round-trip
        /// without invoking the full `Xpt2046` (which needs an SPI/CS).
        fn map_range(val: u16, in_min: u16, in_max: u16, out_min: u16, out_max: u16) -> u16 {
            let (lo, hi) = if in_min <= in_max {
                (in_min, in_max)
            } else {
                (in_max, in_min)
            };
            let val = val.clamp(lo, hi) as i32;
            let in_min = in_min as i32;
            let in_max = in_max as i32;
            let in_range = in_max - in_min;
            let out_range = out_max as i32 - out_min as i32;
            if in_range == 0 {
                return out_min;
            }
            let result = out_min as i32 + (val - in_min) * out_range / in_range;
            result.clamp(out_min as i32, out_max as i32) as u16
        }

        fn round_trip(s: u16, cal_min: u16, cal_max: u16, screen: u16) -> u16 {
            let raw = FakeXptSpi::screen_to_raw(s, cal_min, cal_max, screen);
            map_range(raw, cal_min, cal_max, 0, screen - 1)
        }

        #[test]
        fn round_trip_inverted_x_axis() {
            // testbench_rp2350 X cal: 1970 → 0, 185 → 319 (inverted).
            for s in [0u16, 1, 50, 159, 160, 161, 250, 318, 319] {
                let back = round_trip(s, 1970, 185, 320);
                assert!(
                    back.abs_diff(s) <= 1,
                    "x={s} round-tripped to {back} (diff={})",
                    back.abs_diff(s)
                );
            }
        }

        #[test]
        fn round_trip_normal_y_axis() {
            // testbench_rp2350 Y cal: 110 → 0, 1950 → 239 (normal).
            for s in [0u16, 1, 50, 119, 120, 121, 200, 238, 239] {
                let back = round_trip(s, 110, 1950, 240);
                assert!(
                    back.abs_diff(s) <= 1,
                    "y={s} round-tripped to {back} (diff={})",
                    back.abs_diff(s)
                );
            }
        }

        #[test]
        fn endpoints_exact() {
            // Endpoints must round-trip exactly — they're the calibration anchors.
            assert_eq!(round_trip(0, 1970, 185, 320), 0);
            assert_eq!(round_trip(319, 1970, 185, 320), 319);
            assert_eq!(round_trip(0, 110, 1950, 240), 0);
            assert_eq!(round_trip(239, 110, 1950, 240), 239);
        }
    }
}

#[cfg(not(has_touch))]
mod inner {
    pub fn init() {
        println!("[sim] Touch: no [touch] in board.toml — disabled");
    }
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
    pub fn inject_override(_: u16, _: u16) {}
    pub fn release_override() {}
    pub fn clear_override() {}
}

pub use inner::*;
