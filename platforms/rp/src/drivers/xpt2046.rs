// SPDX-License-Identifier: GPL-3.0-only
//! Chip-agnostic XPT2046 resistive touch controller driver.
//!
//! Generic over `embedded-hal` traits plus `SpiFreqSwitch` for
//! shared-bus frequency management.

use embedded_hal::digital::OutputPin;
use embedded_hal::spi::SpiBus;

use super::SpiFreqSwitch;

// XPT2046 control bytes
const CMD_READ_X: u8 = 0xD0; // X position, 12-bit, differential
const CMD_READ_Y: u8 = 0x90; // Y position, 12-bit, differential

/// Number of samples per read. Highest and lowest are discarded;
/// the rest are averaged to eliminate transient spikes.
const NUM_SAMPLES: usize = 5;

pub struct Xpt2046<SPI, CS> {
    spi: SPI,
    cs: CS,
    touch_spi_freq: u32,
    display_spi_freq: u32,
    screen_width: u16,
    screen_height: u16,
    cal_x_min: u16,
    cal_x_max: u16,
    cal_y_min: u16,
    cal_y_max: u16,
    /// Noise rejection: raw values below this on either axis → no touch.
    reject_lo: u16,
    /// Noise rejection: raw values above this on either axis → no touch.
    reject_hi: u16,
    /// Swap raw X/Y axes before calibration mapping (board-dependent).
    swap_xy: bool,
}

impl<SPI, CS> Xpt2046<SPI, CS>
where
    SPI: SpiBus + SpiFreqSwitch,
    CS: OutputPin,
{
    /// Create a new XPT2046 driver.
    ///
    /// * `touch_spi_freq` — SPI clock for touch reads (max ~2.5 MHz)
    /// * `display_spi_freq` — SPI clock to restore after touch reads
    /// * `cal_*` — raw ADC range for calibration mapping
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spi: SPI,
        cs: CS,
        touch_spi_freq: u32,
        display_spi_freq: u32,
        screen_width: u16,
        screen_height: u16,
        cal_x_min: u16,
        cal_x_max: u16,
        cal_y_min: u16,
        cal_y_max: u16,
    ) -> Self {
        Self {
            spi,
            cs,
            touch_spi_freq,
            display_spi_freq,
            screen_width,
            screen_height,
            cal_x_min,
            cal_x_max,
            cal_y_min,
            cal_y_max,
            reject_lo: 100,
            reject_hi: 4000,
            swap_xy: false,
        }
    }

    /// Send an initial command to enable PENIRQ output.
    /// Call after construction to activate the touch controller.
    pub fn init(&mut self) {
        self.spi.set_frequency(self.touch_spi_freq);
        let _ = self.cs.set_low();
        let tx = [CMD_READ_Y, 0x00, 0x00];
        let mut rx = [0u8; 3];
        let _ = self.spi.transfer(&mut rx, &tx);
        let _ = self.cs.set_high();
        self.spi.set_frequency(self.display_spi_freq);
    }

    /// Read one raw 12-bit sample for a given command byte.
    fn read_one(&mut self, cmd: u8) -> u16 {
        let tx = [cmd, 0x00, 0x00];
        let mut rx = [0u8; 3];
        let _ = self.spi.transfer(&mut rx, &tx);
        ((rx[1] as u16) << 4) | ((rx[2] as u16) >> 4)
    }

    /// Update calibration constants at runtime (e.g. after a calibration routine).
    pub fn set_calibration(
        &mut self,
        cal_x_min: u16,
        cal_x_max: u16,
        cal_y_min: u16,
        cal_y_max: u16,
    ) {
        self.cal_x_min = cal_x_min;
        self.cal_x_max = cal_x_max;
        self.cal_y_min = cal_y_min;
        self.cal_y_max = cal_y_max;
    }

    /// Update noise-rejection thresholds at runtime.
    ///
    /// Raw values outside `lo..=hi` on either axis are treated as "no touch".
    pub fn set_rejection_range(&mut self, lo: u16, hi: u16) {
        self.reject_lo = lo;
        self.reject_hi = hi;
    }

    /// Enable or disable raw X/Y axis swapping.
    ///
    /// Some boards mount the touch panel rotated relative to the display;
    /// enabling swap corrects this so raw X maps to screen X.
    pub fn set_swap_xy(&mut self, swap: bool) {
        self.swap_xy = swap;
    }

    /// Multi-sample averaged read (no rejection filter).
    ///
    /// Takes `NUM_SAMPLES` readings, discards the highest and lowest,
    /// and returns the average of the remaining samples on each axis.
    fn sample(&mut self) -> (u16, u16) {
        self.spi.set_frequency(self.touch_spi_freq);
        let _ = self.cs.set_low();

        let mut xs = [0u16; NUM_SAMPLES];
        let mut ys = [0u16; NUM_SAMPLES];
        for i in 0..NUM_SAMPLES {
            xs[i] = self.read_one(CMD_READ_X);
            ys[i] = self.read_one(CMD_READ_Y);
        }

        let _ = self.cs.set_high();
        self.spi.set_frequency(self.display_spi_freq);

        xs.sort_unstable();
        ys.sort_unstable();
        let mid = &xs[1..NUM_SAMPLES - 1];
        let raw_x = (mid.iter().map(|&v| v as u32).sum::<u32>() / mid.len() as u32) as u16;
        let mid = &ys[1..NUM_SAMPLES - 1];
        let raw_y = (mid.iter().map(|&v| v as u32).sum::<u32>() / mid.len() as u32) as u16;

        if self.swap_xy {
            (raw_y, raw_x)
        } else {
            (raw_x, raw_y)
        }
    }

    /// Read raw 12-bit X and Y without noise rejection.
    ///
    /// Useful for noise-floor discovery during calibration.
    pub fn read_raw_unfiltered(&mut self) -> (u16, u16) {
        self.sample()
    }

    /// Read raw 12-bit X and Y with multi-sample averaging and noise rejection.
    pub fn read_raw(&mut self) -> Option<(u16, u16)> {
        let (raw_x, raw_y) = self.sample();

        if !(self.reject_lo..=self.reject_hi).contains(&raw_x)
            || !(self.reject_lo..=self.reject_hi).contains(&raw_y)
        {
            return None;
        }

        Some((raw_x, raw_y))
    }

    /// Map a value from one range to another.
    ///
    /// Handles inverted input ranges (in_min > in_max) so that calibration
    /// works even when a raw touch axis runs opposite to the screen axis.
    fn map_range(val: u16, in_min: u16, in_max: u16, out_min: u16, out_max: u16) -> u16 {
        let (lo, hi) = if in_min <= in_max {
            (in_min, in_max)
        } else {
            (in_max, in_min)
        };
        let val = val.clamp(lo, hi) as i32;
        let in_min = in_min as i32;
        let in_max = in_max as i32;
        let in_range = in_max - in_min; // may be negative
        let out_range = out_max as i32 - out_min as i32;
        if in_range == 0 {
            return out_min;
        }
        let result = out_min as i32 + (val - in_min) * out_range / in_range;
        result.clamp(out_min as i32, out_max as i32) as u16
    }

    /// Read calibrated screen coordinates.
    /// Returns `None` if no touch is active.
    pub fn read_point(&mut self) -> Option<(u16, u16)> {
        let (raw_x, raw_y) = self.read_raw()?;
        let screen_x = Self::map_range(
            raw_x,
            self.cal_x_min,
            self.cal_x_max,
            0,
            self.screen_width - 1,
        );
        let screen_y = Self::map_range(
            raw_y,
            self.cal_y_min,
            self.cal_y_max,
            0,
            self.screen_height - 1,
        );
        Some((screen_x, screen_y))
    }
}
