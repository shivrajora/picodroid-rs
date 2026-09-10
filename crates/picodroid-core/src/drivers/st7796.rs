// SPDX-License-Identifier: GPL-3.0-only
//! Chip-agnostic ST7796 TFT display driver.
//!
//! Generic over `embedded-hal` traits — any MCU that provides `SpiBus`,
//! `OutputPin`, and `DelayNs` can use this driver.
//!
//! The ST7796 shares its whole runtime command set with the ST7789 in
//! [`super::st7789`]: CASET, RASET and RAMWR address and fill the window,
//! MADCTL sets orientation, COLMOD sets the pixel format, and SLPIN/SLPOUT/
//! DISPON/DISPOFF drive power. Only `init` differs, and it differs a lot — the
//! ST7796 wants a power/VCOM/gamma block the ST7789 does not, and it comes up
//! with the wrong polarity unless inversion is turned on at the end. The two
//! drivers are separate rather than one parameterised type because that init
//! block is most of what a panel driver *is*; sharing the eleven trivial
//! wrappers would not pay for the indirection through the init table.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::SpiBus;

// Commands shared with the ST7789.
const CMD_SLPIN: u8 = 0x10;
const CMD_SLPOUT: u8 = 0x11;
const CMD_INVON: u8 = 0x21;
const CMD_DISPOFF: u8 = 0x28;
const CMD_DISPON: u8 = 0x29;
const CMD_CASET: u8 = 0x2A;
const CMD_RASET: u8 = 0x2B;
const CMD_RAMWR: u8 = 0x2C;
const CMD_MADCTL: u8 = 0x36;
const CMD_COLMOD: u8 = 0x3A;

/// 16-bit RGB565. The ST7796's COLMOD takes 0x05 here, not the ST7789's 0x55:
/// the field is 3 bits wide on this controller and the upper nibble is
/// reserved.
const COLMOD_RGB565: u8 = 0x05;

/// Positive and negative gamma correction, 15 bytes each. Named rather than
/// inlined so [`INIT_SEQUENCE`] stays one readable command per line.
const POSITIVE_GAMMA: [u8; 15] = [
    0x1F, 0x1A, 0x18, 0x0A, 0x0F, 0x06, 0x45, 0x87, 0x32, 0x0A, 0x07, 0x02, 0x07, 0x05, 0x00,
];
const NEGATIVE_GAMMA: [u8; 15] = [
    0x00, 0x25, 0x27, 0x05, 0x10, 0x09, 0x3A, 0x78, 0x4D, 0x05, 0x18, 0x0D, 0x38, 0x3A, 0x1F,
];

/// The power, VCOM and gamma block this panel needs before it will show a
/// correct picture, as `(command, data)` pairs.
///
/// Taken from the GeeekPi reference firmware for the 52Pi EP-0172, which is the
/// only 320x480 ST7796 module picodroid targets. It is not the ST7796
/// datasheet's own suggested sequence: several of these registers (0xCF, 0xED,
/// 0xCB, 0xF7, 0xEA) are ILI9341 commands that this controller tolerates and
/// the vendor never removed. They are kept verbatim rather than cleaned up
/// because the vendor sequence is what is known to drive this glass, and a
/// tidier one is a bet against a panel nobody here can re-tune.
const INIT_SEQUENCE: &[(u8, &[u8])] = &[
    (0xCF, &[0x00, 0x83, 0x30]),
    (0xED, &[0x64, 0x03, 0x12, 0x81]),
    (0xE8, &[0x85, 0x01, 0x79]),
    (0xCB, &[0x39, 0x2C, 0x00, 0x34, 0x02]),
    (0xF7, &[0x20]),
    (0xEA, &[0x00, 0x00]),
    (0xC0, &[0x26]),       // power control 1
    (0xC1, &[0x11]),       // power control 2
    (0xC5, &[0x35, 0x3E]), // VCOM control 1
    (0xC7, &[0xBE]),       // VCOM control 2
    (0xB1, &[0x00, 0x1B]), // frame rate
    (0xF2, &[0x08]),
    (0x26, &[0x01]), // gamma curve select
    (0xE0, &POSITIVE_GAMMA),
    (0xE1, &NEGATIVE_GAMMA),
    (0xB7, &[0x07]),                   // entry mode
    (0xB6, &[0x0A, 0x82, 0x27, 0x00]), // display function control
];

pub struct St7796<SPI, DC, CS, RST, BL, D> {
    spi: SPI,
    dc: DC,
    cs: CS,
    rst: RST,
    bl: BL,
    delay: D,
    width: u16,
    height: u16,
    madctl: u8,
}

impl<SPI, DC, CS, RST, BL, D> St7796<SPI, DC, CS, RST, BL, D>
where
    SPI: SpiBus,
    DC: OutputPin,
    CS: OutputPin,
    RST: OutputPin,
    BL: OutputPin,
    D: DelayNs,
{
    /// Create a new ST7796 driver. Does NOT initialize the display —
    /// call `init()` after construction.
    ///
    /// `madctl` sets the MADCTL register for display orientation/mirroring.
    /// Common values on a 320x480 module: 0x48 = portrait 320x480 (BGR),
    /// 0x28 = landscape 480x320.
    ///
    /// `rst` and `bl` may be pins that do nothing: a module can tie either line
    /// high in hardware, and the 52Pi EP-0172 does exactly that with its
    /// backlight.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spi: SPI,
        dc: DC,
        cs: CS,
        rst: RST,
        bl: BL,
        delay: D,
        width: u16,
        height: u16,
        madctl: u8,
    ) -> Self {
        Self {
            spi,
            dc,
            cs,
            rst,
            bl,
            delay,
            width,
            height,
            madctl,
        }
    }

    fn write_command(&mut self, cmd: u8) {
        let _ = self.cs.set_low();
        let _ = self.dc.set_low(); // command mode
        let _ = self.spi.write(&[cmd]);
        let _ = self.cs.set_high();
    }

    fn write_command_data(&mut self, cmd: u8, data: &[u8]) {
        let _ = self.cs.set_low();
        let _ = self.dc.set_low();
        let _ = self.spi.write(&[cmd]);
        let _ = self.dc.set_high();
        let _ = self.spi.write(data);
        let _ = self.cs.set_high();
    }

    /// Run the ST7796 initialization sequence (hardware reset + register config).
    pub fn init(&mut self) {
        // Hardware reset. The vendor firmware idles RST high for 100 ms before
        // pulsing it, rather than assuming the pin's power-on state; on a
        // module that ties RST high this whole block is inert.
        let _ = self.rst.set_high();
        self.delay.delay_ms(100);
        let _ = self.rst.set_low();
        self.delay.delay_ms(100);
        let _ = self.rst.set_high();
        self.delay.delay_ms(100);

        for (cmd, data) in INIT_SEQUENCE {
            if data.is_empty() {
                self.write_command(*cmd);
            } else {
                self.write_command_data(*cmd, data);
            }
        }

        // Color mode: 16-bit RGB565.
        self.write_command_data(CMD_COLMOD, &[COLMOD_RGB565]);

        // Memory data access control (orientation set by board config).
        self.write_command_data(CMD_MADCTL, &[self.madctl]);

        // Sleep out, then display on. Both need the datasheet's 120 ms before
        // anything else is sent.
        self.write_command(CMD_SLPOUT);
        self.delay.delay_ms(120);
        self.write_command(CMD_DISPON);
        self.delay.delay_ms(120);

        // Inversion on. This panel is normally-black wired the other way up;
        // without it every color comes out complemented.
        self.write_command(CMD_INVON);
    }

    /// Set the active drawing window.
    pub fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) {
        self.write_command_data(
            CMD_CASET,
            &[
                (x0 >> 8) as u8,
                (x0 & 0xFF) as u8,
                (x1 >> 8) as u8,
                (x1 & 0xFF) as u8,
            ],
        );
        self.write_command_data(
            CMD_RASET,
            &[
                (y0 >> 8) as u8,
                (y0 & 0xFF) as u8,
                (y1 >> 8) as u8,
                (y1 & 0xFF) as u8,
            ],
        );
        self.write_command(CMD_RAMWR);
    }

    /// Stream RGB565 pixel data to the display within the current window.
    pub fn write_pixels(&mut self, data: &[u8]) {
        let _ = self.cs.set_low();
        let _ = self.dc.set_high(); // data mode
        let _ = self.spi.write(data);
        let _ = self.cs.set_high();
    }

    /// Turn the backlight on or off. A no-op on a module with no backlight pin,
    /// where the caller's blanking still reaches the panel through
    /// [`Self::display_off`].
    pub fn set_backlight(&mut self, on: bool) {
        if on {
            let _ = self.bl.set_high();
        } else {
            let _ = self.bl.set_low();
        }
    }

    /// Enter low-power sleep mode. Datasheet requires ~5 ms before subsequent
    /// commands and ~120 ms before the next SLPOUT.
    pub fn sleep_in(&mut self) {
        self.write_command(CMD_SLPIN);
        self.delay.delay_ms(5);
    }

    /// Leave low-power sleep mode. Datasheet mandates a 120 ms wait before any
    /// further commands (matches the init sequence at startup).
    pub fn sleep_out(&mut self) {
        self.write_command(CMD_SLPOUT);
        self.delay.delay_ms(120);
    }

    /// Blank the display (panel RAM is retained).
    pub fn display_off(&mut self) {
        self.write_command(CMD_DISPOFF);
    }

    /// Show the display (after a prior `display_off` or fresh `sleep_out`).
    pub fn display_on(&mut self) {
        self.write_command(CMD_DISPON);
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::spi::ErrorType;

    /// What the driver did, in order. `Cmd` carries the command byte and the
    /// data that followed it; `Pin` records a level change on rst or bl.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Step {
        Cmd(u8, Vec<u8>),
        Rst(bool),
        Bl(bool),
    }

    /// Shared recorder. The driver owns the bus and every pin separately, so
    /// they need one log between them to preserve ordering.
    #[derive(Default, Clone)]
    struct Log(std::rc::Rc<std::cell::RefCell<Vec<Step>>>);

    impl Log {
        fn push(&self, s: Step) {
            self.0.borrow_mut().push(s);
        }
        fn steps(&self) -> Vec<Step> {
            self.0.borrow().clone()
        }
        /// The data bytes that followed `cmd`, the first time it was sent.
        fn data_for(&self, cmd: u8) -> Option<Vec<u8>> {
            self.steps().into_iter().find_map(|s| match s {
                Step::Cmd(c, d) if c == cmd => Some(d),
                _ => None,
            })
        }
        /// Index of the first `cmd` in the step list.
        fn pos_of(&self, cmd: u8) -> Option<usize> {
            self.steps()
                .iter()
                .position(|s| matches!(s, Step::Cmd(c, _) if *c == cmd))
        }
    }

    /// Records the driver's `write_command` / `write_command_data` framing by
    /// watching DC alongside the bytes: DC low is a command, DC high is its
    /// data. That is the actual wire contract, so a driver that stopped
    /// toggling DC would fail here rather than pass on byte order alone.
    struct FakeSpi {
        log: Log,
        dc_high: std::rc::Rc<std::cell::Cell<bool>>,
    }

    impl ErrorType for FakeSpi {
        type Error = Infallible;
    }

    impl SpiBus<u8> for FakeSpi {
        fn read(&mut self, _w: &mut [u8]) -> Result<(), Infallible> {
            Ok(())
        }
        fn write(&mut self, words: &[u8]) -> Result<(), Infallible> {
            if self.dc_high.get() {
                // Data for whichever command was last logged.
                if let Some(Step::Cmd(_, data)) = self.log.0.borrow_mut().last_mut() {
                    data.extend_from_slice(words);
                }
            } else {
                for &b in words {
                    self.log.push(Step::Cmd(b, Vec::new()));
                }
            }
            Ok(())
        }
        fn transfer(&mut self, _rx: &mut [u8], _tx: &[u8]) -> Result<(), Infallible> {
            Ok(())
        }
        fn transfer_in_place(&mut self, _w: &mut [u8]) -> Result<(), Infallible> {
            Ok(())
        }
        fn flush(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
    }

    /// A pin that only tracks its level (DC and CS), for the SPI fake to read.
    struct LevelPin(std::rc::Rc<std::cell::Cell<bool>>);

    impl embedded_hal::digital::ErrorType for LevelPin {
        type Error = Infallible;
    }

    impl OutputPin for LevelPin {
        fn set_low(&mut self) -> Result<(), Infallible> {
            self.0.set(false);
            Ok(())
        }
        fn set_high(&mut self) -> Result<(), Infallible> {
            self.0.set(true);
            Ok(())
        }
    }

    /// A pin whose every transition is recorded, for rst and bl.
    struct RecordedPin {
        log: Log,
        make: fn(bool) -> Step,
    }

    impl embedded_hal::digital::ErrorType for RecordedPin {
        type Error = Infallible;
    }

    impl OutputPin for RecordedPin {
        fn set_low(&mut self) -> Result<(), Infallible> {
            self.log.push((self.make)(false));
            Ok(())
        }
        fn set_high(&mut self) -> Result<(), Infallible> {
            self.log.push((self.make)(true));
            Ok(())
        }
    }

    struct NoDelay;
    impl DelayNs for NoDelay {
        fn delay_ns(&mut self, _ns: u32) {}
    }

    type TestPanel = St7796<FakeSpi, LevelPin, LevelPin, RecordedPin, RecordedPin, NoDelay>;

    fn panel(madctl: u8) -> (TestPanel, Log) {
        let log = Log::default();
        let dc = std::rc::Rc::new(std::cell::Cell::new(false));
        let cs = std::rc::Rc::new(std::cell::Cell::new(true));
        let spi = FakeSpi {
            log: log.clone(),
            dc_high: dc.clone(),
        };
        let p = St7796::new(
            spi,
            LevelPin(dc),
            LevelPin(cs),
            RecordedPin {
                log: log.clone(),
                make: Step::Rst,
            },
            RecordedPin {
                log: log.clone(),
                make: Step::Bl,
            },
            NoDelay,
            320,
            480,
            madctl,
        );
        (p, log)
    }

    #[test]
    fn init_pulses_reset_low_between_two_highs() {
        let (mut p, log) = panel(0x48);
        p.init();
        let pulses: Vec<bool> = log
            .steps()
            .into_iter()
            .filter_map(|s| match s {
                Step::Rst(level) => Some(level),
                _ => None,
            })
            .collect();
        assert_eq!(pulses, vec![true, false, true]);
    }

    #[test]
    fn init_sets_rgb565_and_the_boards_madctl() {
        let (mut p, log) = panel(0x48);
        p.init();
        // 0x05, not the ST7789's 0x55: the field is 3 bits wide here.
        assert_eq!(log.data_for(CMD_COLMOD), Some(vec![COLMOD_RGB565]));
        assert_eq!(log.data_for(CMD_MADCTL), Some(vec![0x48]));

        let (mut landscape, log2) = panel(0x28);
        landscape.init();
        assert_eq!(log2.data_for(CMD_MADCTL), Some(vec![0x28]));
    }

    #[test]
    fn init_sends_the_vendor_sequence_before_sleep_out() {
        let (mut p, log) = panel(0x48);
        p.init();
        let slpout = log.pos_of(CMD_SLPOUT).expect("sleep out was sent");
        for (cmd, _) in INIT_SEQUENCE {
            let at = log
                .pos_of(*cmd)
                .unwrap_or_else(|| panic!("init never sent {cmd:#04x}"));
            assert!(
                at < slpout,
                "{cmd:#04x} must be configured before SLPOUT, was at {at} vs {slpout}"
            );
        }
    }

    #[test]
    fn init_carries_both_gamma_tables_intact() {
        let (mut p, log) = panel(0x48);
        p.init();
        assert_eq!(log.data_for(0xE0), Some(POSITIVE_GAMMA.to_vec()));
        assert_eq!(log.data_for(0xE1), Some(NEGATIVE_GAMMA.to_vec()));
    }

    #[test]
    fn init_ends_with_display_on_then_inversion() {
        let (mut p, log) = panel(0x48);
        p.init();
        let dispon = log.pos_of(CMD_DISPON).expect("display on was sent");
        let invon = log.pos_of(CMD_INVON).expect("inversion was sent");
        let slpout = log.pos_of(CMD_SLPOUT).expect("sleep out was sent");
        assert!(slpout < dispon, "SLPOUT precedes DISPON");
        // Inversion last: this panel comes up with the polarity reversed, and
        // turning it on earlier is undone by the power-up sequence.
        assert!(dispon < invon, "INVON is the final command");
    }

    #[test]
    fn set_window_sends_both_ranges_big_endian_then_ramwr() {
        let (mut p, log) = panel(0x48);
        p.set_window(1, 2, 0x0140, 0x01E0);
        assert_eq!(log.data_for(CMD_CASET), Some(vec![0x00, 0x01, 0x01, 0x40]));
        assert_eq!(log.data_for(CMD_RASET), Some(vec![0x00, 0x02, 0x01, 0xE0]));
        let caset = log.pos_of(CMD_CASET).unwrap();
        let raset = log.pos_of(CMD_RASET).unwrap();
        let ramwr = log.pos_of(CMD_RAMWR).expect("RAMWR was sent");
        assert!(caset < raset && raset < ramwr, "CASET, RASET, then RAMWR");
    }

    #[test]
    fn the_backlight_pin_follows_the_request() {
        let (mut p, log) = panel(0x48);
        p.set_backlight(true);
        p.set_backlight(false);
        let levels: Vec<bool> = log
            .steps()
            .into_iter()
            .filter_map(|s| match s {
                Step::Bl(level) => Some(level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, vec![true, false]);
    }

    #[test]
    fn sleep_and_blank_send_their_own_commands() {
        let (mut p, log) = panel(0x48);
        p.sleep_in();
        p.sleep_out();
        p.display_off();
        p.display_on();
        let cmds: Vec<u8> = log
            .steps()
            .into_iter()
            .filter_map(|s| match s {
                Step::Cmd(c, _) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(cmds, vec![CMD_SLPIN, CMD_SLPOUT, CMD_DISPOFF, CMD_DISPON]);
    }

    #[test]
    fn geometry_is_what_the_board_asked_for() {
        let (p, _) = panel(0x48);
        assert_eq!((p.width(), p.height()), (320, 480));
    }
}
