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

/// Bytes on the wire per conversion: the control byte, then the 12-bit
/// result MSB-first across the next two (bits 15..4 of the 16 clocks).
const FRAME: usize = 3;

/// One `sample()` on the wire: X then Y, `NUM_SAMPLES` times, as a single
/// transfer. The chip latches a new control byte on the first clock after
/// a conversion's 24, so back-to-back frames under one CS assertion are
/// exactly what ten separate 3-byte transfers put on the bus — minus the
/// idle gaps, and minus ten trips through the bus driver's polled small
/// path from the UI task (docs/scheduling-audit-2026-09.md, F18). At this
/// length the transfer goes through the driver's interrupt-driven path and
/// the task sleeps while the bytes clock out.
const BATCH: usize = NUM_SAMPLES * 2 * FRAME;

/// The 12-bit result carried by one received frame.
fn decode(frame: &[u8]) -> u16 {
    ((frame[1] as u16) << 4) | ((frame[2] as u16) >> 4)
}

/// Ascending in-place sort for the tiny median-filter sample buffers.
fn insertion_sort_u16(buf: &mut [u16]) {
    for i in 1..buf.len() {
        let key = buf[i];
        let mut j = i;
        while j > 0 && buf[j - 1] > key {
            buf[j] = buf[j - 1];
            j -= 1;
        }
        buf[j] = key;
    }
}

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
    /// Fixed at construction; there is no runtime setter because nothing
    /// ever needed to retune it after `new`.
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

        let mut tx = [0u8; BATCH];
        for pair in tx.chunks_exact_mut(2 * FRAME) {
            pair[0] = CMD_READ_X;
            pair[FRAME] = CMD_READ_Y;
        }
        let mut rx = [0u8; BATCH];
        let _ = self.spi.transfer(&mut rx, &tx);

        let _ = self.cs.set_high();
        self.spi.set_frequency(self.display_spi_freq);

        let mut xs = [0u16; NUM_SAMPLES];
        let mut ys = [0u16; NUM_SAMPLES];
        for (i, pair) in rx.chunks_exact(2 * FRAME).enumerate() {
            xs[i] = decode(&pair[..FRAME]);
            ys[i] = decode(&pair[FRAME..]);
        }

        // Insertion sort, not `sort_unstable`: five elements never reach
        // the point where a real quicksort pays for itself, and the generic
        // sort would monomorphise its whole machinery into the firmware for
        // this one call site. See `pico_jvm::sort` for the same trade.
        insertion_sort_u16(&mut xs);
        insertion_sort_u16(&mut ys);
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::spi::ErrorType;

    /// A bus that answers every frame from a per-command list and records
    /// how many transfers it saw and how long each was.
    struct Bus {
        xs: [u16; NUM_SAMPLES],
        ys: [u16; NUM_SAMPLES],
        transfers: Vec<usize>,
        freqs: Vec<u32>,
    }

    impl SpiFreqSwitch for Bus {
        fn set_frequency(&mut self, freq_hz: u32) {
            self.freqs.push(freq_hz);
        }
    }
    impl ErrorType for Bus {
        type Error = Infallible;
    }
    impl SpiBus<u8> for Bus {
        fn read(&mut self, _: &mut [u8]) -> Result<(), Infallible> {
            Ok(())
        }
        fn write(&mut self, _: &[u8]) -> Result<(), Infallible> {
            Ok(())
        }
        fn transfer(&mut self, rx: &mut [u8], tx: &[u8]) -> Result<(), Infallible> {
            self.transfers.push(tx.len());
            let (mut xi, mut yi) = (0, 0);
            for (i, frame) in tx.chunks_exact(FRAME).enumerate() {
                let raw = match frame[0] {
                    CMD_READ_X => {
                        xi += 1;
                        self.xs[xi - 1]
                    }
                    CMD_READ_Y => {
                        yi += 1;
                        self.ys[yi - 1]
                    }
                    other => panic!("frame {i} sends {other:#04x}, not a read command"),
                };
                assert_eq!(
                    &frame[1..],
                    &[0, 0],
                    "frame {i} must clock zeros after the command"
                );
                rx[i * FRAME + 1] = (raw >> 4) as u8;
                rx[i * FRAME + 2] = ((raw & 0x0F) << 4) as u8;
            }
            Ok(())
        }
        fn transfer_in_place(&mut self, _: &mut [u8]) -> Result<(), Infallible> {
            Ok(())
        }
        fn flush(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
    }

    struct Pin;
    impl embedded_hal::digital::ErrorType for Pin {
        type Error = Infallible;
    }
    impl OutputPin for Pin {
        fn set_low(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
        fn set_high(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
    }

    fn touch(xs: [u16; NUM_SAMPLES], ys: [u16; NUM_SAMPLES]) -> Xpt2046<Bus, Pin> {
        let bus = Bus {
            xs,
            ys,
            transfers: Vec::new(),
            freqs: Vec::new(),
        };
        Xpt2046::new(
            bus, Pin, 2_000_000, 62_500_000, 320, 240, 200, 3900, 200, 3900,
        )
    }

    #[test]
    fn one_sample_is_one_transfer_of_every_frame() {
        let mut t = touch([100, 900, 500, 400, 600], [2000, 2100, 1900, 2050, 1950]);
        let (x, y) = t.read_raw_unfiltered();
        // Highest and lowest dropped, the middle three averaged.
        assert_eq!((x, y), (500, 2000));
        assert_eq!(t.spi.transfers, vec![BATCH], "ten frames, one transfer");
        assert_eq!(
            t.spi.freqs,
            vec![2_000_000, 62_500_000],
            "the bus is switched to the panel's rate once and restored once"
        );
    }

    #[test]
    fn a_batch_is_long_enough_to_leave_the_polled_path() {
        // The RP bus driver polls transfers of at most 8 bytes from the
        // calling task and sleeps on an interrupt for anything longer; a
        // sample must be the latter or the UI task polls the panel again.
        assert!(BATCH > 8);
        assert_eq!(BATCH % FRAME, 0);
    }

    #[test]
    fn rejection_bounds_apply_to_the_averaged_value() {
        let mut t = touch([0; NUM_SAMPLES], [2000; NUM_SAMPLES]);
        assert_eq!(t.read_raw(), None, "raw 0 is below reject_lo");
        let mut t = touch([1000; NUM_SAMPLES], [2000; NUM_SAMPLES]);
        assert_eq!(t.read_raw(), Some((1000, 2000)));
    }
}
