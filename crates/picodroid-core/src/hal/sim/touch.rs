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
//! Round-tripping through the driver rather than faking screen coordinates is
//! the point: a calibration or swap_xy bug shows up in the simulator instead
//! of waiting for hardware.

#[cfg(all(has_touch, touch_xpt2046))]
mod inner {
    use super::super::output_pin::SimOutputPin;
    use crate::drivers::xpt2046::Xpt2046;
    use crate::drivers::SpiFreqSwitch;
    use core::convert::Infallible;
    use core::ptr::addr_of_mut;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use embedded_hal::spi::{ErrorType, SpiBus};

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

    impl Default for FakeXptSpi {
        fn default() -> Self {
            Self::new()
        }
    }

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

    pub fn read_raw_unfiltered() -> (u16, u16) {
        touch().read_raw_unfiltered()
    }

    pub fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16) {
        touch().set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max);
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

// ── GT911 (capacitive, I2C) ─────────────────────────────────────────────────

/// Simulator backend for a `[touch] driver = "gt911"` board.
///
/// Same principle as the XPT2046 arm above: the mouse is fed through the *real*
/// [`Gt911`] driver over a fake bus, so the register framing, the status
/// handshake and the axis transform all run in the simulator exactly as they do
/// on hardware. A swapped axis or a missing status clear shows up here rather
/// than waiting for the bench.
///
/// What it does not model is the reset that latches the bus address. That is
/// pure pin-wiggling with no bus traffic and no observable result on a part
/// that does not exist, and calling it would only spend the datasheet's ~180 ms
/// of delays at every simulator boot.
#[cfg(all(has_touch, touch_gt911))]
mod inner {
    use crate::drivers::gt911::{Gt911, I2cBus};
    use core::ptr::addr_of_mut;

    // board.toml lists every pin/geometry field the panel has; a given build
    // consumes only the subset its code path touches. Scoped here rather than
    // over the whole HAL module so real rot outside the generated table stays
    // visible.
    #[allow(dead_code)]
    mod generated {
        include!(concat!(env!("OUT_DIR"), "/touch_config.rs"));
    }
    #[allow(dead_code)]
    mod display_generated {
        include!(concat!(env!("OUT_DIR"), "/display_config.rs"));
    }

    // Register addresses the fake serves. Kept here rather than imported so
    // the fake is a black-box model of the part: if the driver's map drifts
    // from the datasheet, these stop agreeing and the tests fail.
    const REG_PRODUCT_ID: u16 = 0x8140;
    const REG_STATUS: u16 = 0x814E;
    const REG_POINT1: u16 = 0x8150;

    const STATUS_BUFFER_READY: u8 = 0x80;

    /// Fake I2C device that answers as a GT911 would, from the mouse position.
    ///
    /// The driver's read is two transfers — write the 16-bit register address,
    /// then read N bytes — so the bus has to remember which register was
    /// selected, exactly as the real part does.
    pub struct FakeGt911I2c {
        selected: u16,
    }

    impl Default for FakeGt911I2c {
        fn default() -> Self {
            Self::new()
        }
    }

    impl FakeGt911I2c {
        pub const fn new() -> Self {
            Self { selected: 0 }
        }

        /// The coordinates the controller would report for the current mouse
        /// position — that is, *before* the driver's own swap and clamp. When
        /// the board declares `swap_xy`, the driver will transpose what it
        /// reads, so the fake pre-transposes to keep the round trip honest.
        fn controller_coords() -> (u16, u16) {
            let (_, mx, my) = super::super::display::mouse_state();
            if generated::TOUCH_SWAP_XY {
                (my, mx)
            } else {
                (mx, my)
            }
        }
    }

    impl I2cBus for FakeGt911I2c {
        fn write(&mut self, _addr: u8, data: &[u8]) -> i32 {
            if data.len() >= 2 {
                self.selected = u16::from_be_bytes([data[0], data[1]]);
            }
            // Anything past the address is a register write. The only one the
            // driver makes is the status clear, which needs no state here: the
            // next status read is recomputed from the mouse regardless.
            0
        }

        fn read(&mut self, _addr: u8, buf: &mut [u8]) -> i32 {
            let fill = |buf: &mut [u8], src: &[u8]| {
                let n = buf.len().min(src.len());
                buf[..n].copy_from_slice(&src[..n]);
            };
            match self.selected {
                REG_PRODUCT_ID => {
                    // Product id, firmware, X/Y resolution, vendor — the
                    // contiguous block the driver reads at init. The
                    // resolution is the board's, so the identity the sim
                    // reports is the one hardware should report too.
                    let x = display_generated::SCREEN_WIDTH.to_le_bytes();
                    let y = display_generated::SCREEN_HEIGHT.to_le_bytes();
                    fill(
                        buf,
                        &[
                            b'9', b'1', b'1', 0x00, 0x60, 0x10, x[0], x[1], y[0], y[1], 0x01,
                        ],
                    )
                }
                REG_STATUS => {
                    let (pressed, _, _) = super::super::display::mouse_state();
                    // Always "buffer ready": the point count is what says
                    // whether a finger is down.
                    let points = if pressed { 1 } else { 0 };
                    fill(buf, &[STATUS_BUFFER_READY | points]);
                }
                REG_POINT1 => {
                    let (x, y) = Self::controller_coords();
                    let x = x.to_le_bytes();
                    let y = y.to_le_bytes();
                    fill(buf, &[x[0], x[1], y[0], y[1]]);
                }
                _ => buf.fill(0),
            }
            0
        }
    }

    type Touch = Gt911<FakeGt911I2c>;
    static mut TOUCH: Option<Touch> = None;

    pub fn init() {
        let mut touch = Gt911::new(
            FakeGt911I2c::new(),
            generated::TOUCH_ADDR,
            display_generated::SCREEN_WIDTH,
            display_generated::SCREEN_HEIGHT,
            generated::TOUCH_SWAP_XY,
        );
        // The fake always answers, so a failure here means the driver's own
        // framing broke — worth surfacing loudly in the sim.
        match touch.init() {
            Ok(id) => println!(
                "[sim] Touch: GT911 fw={:#06x} vendor={:#04x} panel={}x{}",
                id.firmware, id.vendor, id.x_resolution, id.y_resolution
            ),
            Err(e) => println!("[sim] Touch: GT911 model rejected its own product id: {e:?}"),
        }
        unsafe {
            addr_of_mut!(TOUCH).write(Some(touch));
        }
        println!(
            "[sim] Touch: GT911 driver active (addr={:#04x}, swap_xy={})",
            generated::TOUCH_ADDR,
            generated::TOUCH_SWAP_XY,
        );
    }

    fn touch() -> &'static mut Touch {
        unsafe { (*addr_of_mut!(TOUCH)).as_mut().unwrap() }
    }

    pub fn read_point() -> Option<(u16, u16)> {
        touch().read_point()
    }

    pub fn read_raw_unfiltered() -> (u16, u16) {
        touch().read_raw_unfiltered()
    }

    /// Nothing to calibrate on a capacitive panel — see the note on the
    /// hardware facade's copy of this.
    pub fn set_calibration(_: u16, _: u16, _: u16, _: u16) {}

    // Scripted-touch override, as on the XPT2046 arm: the display's override
    // machinery feeds `mouse_state()`, so an injected tap runs the whole
    // FakeGt911I2c → Gt911 pipeline rather than short-circuiting it.
    pub fn inject_override(x: u16, y: u16) {
        super::super::display::set_touch_override(x, y);
    }
    pub fn release_override() {
        super::super::display::touch_override_release();
    }
    pub fn clear_override() {
        super::super::display::clear_touch_override();
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
    pub fn read_raw_unfiltered() -> (u16, u16) {
        (0, 0)
    }
    pub fn set_calibration(_: u16, _: u16, _: u16, _: u16) {}
    pub fn inject_override(_: u16, _: u16) {}
    pub fn release_override() {}
    pub fn clear_override() {}
}

pub use inner::*;
