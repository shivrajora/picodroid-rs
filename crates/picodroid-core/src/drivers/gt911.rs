// SPDX-License-Identifier: GPL-3.0-only
//! Goodix GT911 capacitive touch controller driver.
//!
//! Generic over `I2cBus` so the same code runs against a family HAL and the
//! simulator's fake bus, matching the pattern in [`crate::drivers::ltr559`].
//!
//! Unlike the resistive [`crate::drivers::xpt2046`], this part reports finished
//! pixel coordinates: the controller does its own sampling, filtering and
//! scaling, so there is no calibration to invert, no median filter to run and
//! no `cal_*` bounds in board.toml. What the driver does is the I2C framing
//! (16-bit big-endian register addresses, which nothing else in this tree
//! uses), the power-on reset that picks the bus address, and the status-clear
//! handshake the controller needs after every read.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;

/// Minimal I2C bus trait for the GT911 driver. Both methods return a negative
/// value on a bus error, matching `HalI2c::write_slice` / `read_slice`.
pub trait I2cBus {
    fn write(&mut self, addr: u8, data: &[u8]) -> i32;
    fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32;
}

/// What the controller reports about itself at [`Gt911::init`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// Firmware version, as the part reports it.
    pub firmware: u16,
    /// The controller's own configured panel size. Should match board.toml's
    /// `[display] width` / `height`; when it does not, the touch scale is
    /// wrong even though every driver is behaving.
    pub x_resolution: u16,
    pub y_resolution: u16,
    pub vendor: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gt911Error {
    /// The four product-id bytes were not "911\0" — wrong part, wrong address,
    /// or the reset never released.
    ProductIdMismatch([u8; 4]),
    I2cError,
}

// Register map (GT911 programming guide rev 0.1). Addresses are 16-bit.
const REG_PRODUCT_ID: u16 = 0x8140; // 4 bytes, ASCII "911\0"
const REG_STATUS: u16 = 0x814E;
const REG_POINT1: u16 = 0x8150; // x_lo, x_hi, y_lo, y_hi, size_lo, size_hi

/// `REG_STATUS` bit 7: the coordinate registers hold a fresh sample.
const STATUS_BUFFER_READY: u8 = 0x80;
/// `REG_STATUS` bits 0-3: how many fingers are down.
const STATUS_POINT_COUNT: u8 = 0x0F;

/// The two addresses a GT911 can answer on. Which one it takes is decided by
/// the level on INT as RST is released — see [`Gt911::reset`].
pub const ADDR_PRIMARY: u8 = 0x5D;
pub const ADDR_SECONDARY: u8 = 0x14;

pub struct Gt911<I: I2cBus> {
    bus: I,
    addr: u8,
    width: u16,
    height: u16,
    swap_xy: bool,
}

impl<I: I2cBus> Gt911<I> {
    /// Drive the power-on reset that selects the controller's I2C address.
    ///
    /// The GT911 latches its address from the INT line at the moment RST is
    /// released: INT low selects [`ADDR_PRIMARY`] (0x5D), INT high selects
    /// [`ADDR_SECONDARY`] (0x14). Nothing else can change it afterwards, so
    /// this has to run before the first transfer.
    ///
    /// The caller owns both pins and must reconfigure INT as an input once this
    /// returns — the controller drives it as an interrupt output from then on,
    /// and leaving the MCU driving it fights the part. Taking the pins by
    /// reference rather than storing them is what makes that ordering explicit:
    /// the driver has no way to touch INT later.
    pub fn reset<INT: OutputPin, RST: OutputPin, D: DelayNs>(
        int: &mut INT,
        rst: &mut RST,
        delay: &mut D,
        addr: u8,
    ) {
        // Both lines low: the part is held in reset with the address line
        // parked at the 0x5D level.
        let _ = int.set_low();
        let _ = rst.set_low();
        delay.delay_ms(10);

        // Set INT to the level that selects the address we want, and give it
        // the datasheet's 100 us of setup before RST rises.
        if addr == ADDR_SECONDARY {
            let _ = int.set_high();
        } else {
            let _ = int.set_low();
        }
        delay.delay_us(120);

        // Release reset. The level on INT is sampled here.
        let _ = rst.set_high();
        delay.delay_ms(6);

        // Address is latched; drop INT and let the part boot. It starts
        // driving INT itself once the 50 ms initialisation finishes.
        let _ = int.set_low();
        delay.delay_ms(60);
    }

    /// Wrap an already-reset controller. `width` and `height` are the panel's,
    /// used to clamp what the part reports; `swap_xy` transposes the axes for a
    /// panel mounted rotated relative to the display.
    pub fn new(bus: I, addr: u8, width: u16, height: u16, swap_xy: bool) -> Self {
        Self {
            bus,
            addr,
            width,
            height,
            swap_xy,
        }
    }

    /// Confirm the part is present and talking, and report what it says it is.
    ///
    /// This is the one positive identification of the carrier that costs
    /// nothing: no other board picodroid targets has a GT911, so a controller
    /// answering here *is* the 52Pi EP-0172. Worth logging on a bring-up — the
    /// resolution comes from the controller's own configuration, so comparing
    /// it against board.toml's `width`/`height` catches a panel and a config
    /// that disagree, which otherwise shows up as touches landing at the wrong
    /// scale.
    pub fn init(&mut self) -> Result<Identity, Gt911Error> {
        // Product id, firmware version, resolution and vendor are contiguous
        // from 0x8140, so one read covers the lot.
        let mut info = [0u8; 11];
        self.read_regs(REG_PRODUCT_ID, &mut info)?;
        let id = [info[0], info[1], info[2], info[3]];
        if &id != b"911\0" {
            return Err(Gt911Error::ProductIdMismatch(id));
        }
        Ok(Identity {
            firmware: u16::from_le_bytes([info[4], info[5]]),
            x_resolution: u16::from_le_bytes([info[6], info[7]]),
            y_resolution: u16::from_le_bytes([info[8], info[9]]),
            vendor: info[10],
        })
    }

    /// The first touch point, in screen pixels, or `None` when no finger is
    /// down or the controller has nothing fresh.
    ///
    /// Every call that sees a ready buffer clears the status register, which is
    /// what tells the part it may overwrite the coordinates. Skipping that
    /// leaves the reading frozen at the first sample forever.
    pub fn read_point(&mut self) -> Option<(u16, u16)> {
        let mut status = [0u8; 1];
        if self.read_regs(REG_STATUS, &mut status).is_err() {
            return None;
        }
        if status[0] & STATUS_BUFFER_READY == 0 {
            return None;
        }

        let points = status[0] & STATUS_POINT_COUNT;
        let mut coords = [0u8; 4];
        let read = if points > 0 {
            self.read_regs(REG_POINT1, &mut coords)
        } else {
            Ok(())
        };

        // Clear the buffer-ready flag whatever happened above: a read error
        // that skips this wedges the controller on the next sample too.
        let _ = self.write_regs(REG_STATUS, &[0]);

        if points == 0 || read.is_err() {
            return None;
        }
        Some(self.transform(
            u16::from_le_bytes([coords[0], coords[1]]),
            u16::from_le_bytes([coords[2], coords[3]]),
        ))
    }

    /// The controller's own coordinates for the first point, before the axis
    /// swap and the clamp — the GT911's answer to the XPT2046's raw ADC codes.
    /// `(0, 0)` when nothing is down. Used by the debug bridge's touch report.
    pub fn read_raw_unfiltered(&mut self) -> (u16, u16) {
        let mut coords = [0u8; 4];
        if self.read_regs(REG_POINT1, &mut coords).is_err() {
            return (0, 0);
        }
        (
            u16::from_le_bytes([coords[0], coords[1]]),
            u16::from_le_bytes([coords[2], coords[3]]),
        )
    }

    /// Apply the board's axis swap and clamp into the panel. The controller is
    /// configured for this glass and should already be in range; the clamp is
    /// against a mis-set config, which would otherwise hand LVGL a coordinate
    /// off the end of the screen.
    fn transform(&self, x: u16, y: u16) -> (u16, u16) {
        let (x, y) = if self.swap_xy { (y, x) } else { (x, y) };
        (
            x.min(self.width.saturating_sub(1)),
            y.min(self.height.saturating_sub(1)),
        )
    }

    /// Read `buf.len()` bytes starting at a 16-bit register address. The GT911
    /// wants the address written big-endian as a separate transfer before the
    /// read, which is the one thing that makes it unlike every other I2C part
    /// in this tree.
    fn read_regs(&mut self, reg: u16, buf: &mut [u8]) -> Result<(), Gt911Error> {
        let addr_bytes = [(reg >> 8) as u8, (reg & 0xFF) as u8];
        if self.bus.write(self.addr, &addr_bytes) < 0 {
            return Err(Gt911Error::I2cError);
        }
        if self.bus.read(self.addr, buf) < 0 {
            return Err(Gt911Error::I2cError);
        }
        Ok(())
    }

    /// Write bytes to a 16-bit register address. Only ever one byte today (the
    /// status clear), so the staging buffer is sized for the largest write the
    /// driver makes rather than for the bus.
    fn write_regs(&mut self, reg: u16, data: &[u8]) -> Result<(), Gt911Error> {
        const MAX_WRITE: usize = 4;
        debug_assert!(data.len() <= MAX_WRITE);
        let mut frame = [0u8; 2 + MAX_WRITE];
        frame[0] = (reg >> 8) as u8;
        frame[1] = (reg & 0xFF) as u8;
        let n = data.len().min(MAX_WRITE);
        frame[2..2 + n].copy_from_slice(&data[..n]);
        if self.bus.write(self.addr, &frame[..2 + n]) < 0 {
            return Err(Gt911Error::I2cError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted bus: `reads` is handed out in order, `writes` records what
    /// the driver sent.
    struct FakeBus {
        reads: Vec<Vec<u8>>,
        writes: Vec<Vec<u8>>,
    }

    impl FakeBus {
        fn new(reads: &[&[u8]]) -> Self {
            Self {
                reads: reads.iter().map(|r| r.to_vec()).collect(),
                writes: Vec::new(),
            }
        }
    }

    impl I2cBus for FakeBus {
        fn write(&mut self, _addr: u8, data: &[u8]) -> i32 {
            self.writes.push(data.to_vec());
            0
        }
        fn read(&mut self, _addr: u8, buf: &mut [u8]) -> i32 {
            if self.reads.is_empty() {
                return -1;
            }
            let src = self.reads.remove(0);
            let n = buf.len().min(src.len());
            buf[..n].copy_from_slice(&src[..n]);
            0
        }
    }

    #[test]
    fn init_accepts_the_product_id_and_reports_what_the_part_says_it_is() {
        // "911\0", fw 0x1060, 320x480, vendor 0x01 — the contiguous block from
        // 0x8140 that a real GT911 on the EP-0172 answers with.
        let info: &[u8] = &[
            b'9', b'1', b'1', 0x00, 0x60, 0x10, 0x40, 0x01, 0xE0, 0x01, 0x01,
        ];
        let mut ok = Gt911::new(FakeBus::new(&[info]), ADDR_PRIMARY, 320, 480, false);
        assert_eq!(
            ok.init(),
            Ok(Identity {
                firmware: 0x1060,
                x_resolution: 320,
                y_resolution: 480,
                vendor: 0x01,
            })
        );
    }

    #[test]
    fn init_rejects_a_part_that_is_not_a_gt911() {
        let mut bad = Gt911::new(
            FakeBus::new(&[&[b'9', b'2', b'8', 0x00, 0, 0, 0, 0, 0, 0, 0]]),
            ADDR_PRIMARY,
            320,
            480,
            false,
        );
        assert_eq!(bad.init(), Err(Gt911Error::ProductIdMismatch(*b"928\0")));
    }

    #[test]
    fn a_ready_buffer_with_one_point_reports_its_coordinates() {
        // status: ready + 1 point, then x = 0x0064 (100), y = 0x00C8 (200).
        let bus = FakeBus::new(&[&[STATUS_BUFFER_READY | 1], &[0x64, 0x00, 0xC8, 0x00]]);
        let mut gt = Gt911::new(bus, ADDR_PRIMARY, 320, 480, false);
        assert_eq!(gt.read_point(), Some((100, 200)));
    }

    #[test]
    fn every_ready_read_clears_the_status_register() {
        let bus = FakeBus::new(&[&[STATUS_BUFFER_READY | 1], &[0x64, 0x00, 0xC8, 0x00]]);
        let mut gt = Gt911::new(bus, ADDR_PRIMARY, 320, 480, false);
        let _ = gt.read_point();
        // The last write must be "0 -> 0x814E"; without it the controller never
        // refreshes the coordinates.
        let last = gt.bus.writes.last().expect("a write happened");
        assert_eq!(last.as_slice(), &[0x81, 0x4E, 0x00]);
    }

    #[test]
    fn no_finger_down_reports_nothing_but_still_clears() {
        let bus = FakeBus::new(&[&[STATUS_BUFFER_READY]]); // ready, zero points
        let mut gt = Gt911::new(bus, ADDR_PRIMARY, 320, 480, false);
        assert_eq!(gt.read_point(), None);
        assert_eq!(
            gt.bus.writes.last().expect("a write happened").as_slice(),
            &[0x81, 0x4E, 0x00]
        );
    }

    #[test]
    fn a_stale_buffer_is_not_a_touch() {
        let bus = FakeBus::new(&[&[0x00]]); // buffer-ready clear
        let mut gt = Gt911::new(bus, ADDR_PRIMARY, 320, 480, false);
        assert_eq!(gt.read_point(), None);
        // Nothing to clear, so nothing beyond the register address was written.
        assert!(gt.bus.writes.iter().all(|w| w.len() == 2));
    }

    #[test]
    fn swap_xy_transposes_and_the_clamp_keeps_a_point_on_the_panel() {
        let bus = FakeBus::new(&[&[STATUS_BUFFER_READY | 1], &[0x64, 0x00, 0xC8, 0x00]]);
        let mut gt = Gt911::new(bus, ADDR_PRIMARY, 320, 480, true);
        assert_eq!(gt.read_point(), Some((200, 100)));

        // x = 999 on a 320-wide panel clamps to the last column.
        let bus = FakeBus::new(&[&[STATUS_BUFFER_READY | 1], &[0xE7, 0x03, 0x00, 0x00]]);
        let mut gt = Gt911::new(bus, ADDR_PRIMARY, 320, 480, false);
        assert_eq!(gt.read_point(), Some((319, 0)));
    }

    #[test]
    fn a_read_error_mid_sample_reports_nothing() {
        // Status reads ready with a point, but the coordinate read finds an
        // empty script and fails.
        let bus = FakeBus::new(&[&[STATUS_BUFFER_READY | 1]]);
        let mut gt = Gt911::new(bus, ADDR_PRIMARY, 320, 480, false);
        assert_eq!(gt.read_point(), None);
    }
}
