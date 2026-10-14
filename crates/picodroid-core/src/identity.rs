// SPDX-License-Identifier: GPL-3.0-only
//! The device serial — one value wherever a device is identified, as
//! `ro.serialno` is on Android (docs/designs/device-serial-2026-10.md).
//!
//! It is the USB `iSerialNumber` string ([`crate::pdb::usb_cdc`]), the
//! `picodroid/2.4` PING greeting's serial field, and `Build.getSerial()`.
//! The host's `pdb -s <serial>` finds a device by it before opening any port,
//! and finds it again after a reboot whatever tty or hub port it comes back
//! on.
//!
//! # What a family provides
//!
//! Only how its chip's ID is read and formatted:
//! [`PlatformHooks::device_serial`](crate::host::PlatformHooks::device_serial).
//! Format it the way the vendor's own USB bootloader shows it, so `lsusb`
//! reads the same in both modes (or as the vendor SDK's unique ID, where the
//! bootloader has no per-chip serial) — [`Serial::from_id_bytes`] for the usual
//! uppercase hex of a raw ID, [`Serial::from_ascii`] for anything else:
//!
//! | Family | `device_serial()` |
//! |---|---|
//! | RP2350 | `from_id_bytes` of the OTP chip ID (bootrom `get_sys_info`) |
//! | RP2040 | `from_id_bytes` of the QSPI flash unique ID (`0x4B`) |
//! | STM32 | `from_ascii` of ST's DFU 12-character fold of the 96-bit UID |
//! | nRF52 | `from_id_bytes` of `FICR.DEVICEID` |
//! | simulator | `from_ascii("emulator-<pid>")` |
//!
//! Then call [`init`] once from boot: single-threaded, with the filesystem
//! mounted and before the scheduler, any second core or the USB stack
//! starts. A hook that reads its ID with XIP off (the RP2040) depends on
//! that.
//!
//! Everything else is this module's: a chip with no ID (the hook returns
//! `None`, or an ID that is all `0x00` or all `0xFF`) gets a random serial
//! made once from the family's TRNG ([`crate::hal::entropy`]) and kept in
//! `/system/serial`, so it survives any reflash that keeps the filesystem.
//! With no entropy source either, the serial is [`UNKNOWN`], which the host
//! treats as no serial at all.

use core::sync::atomic::{AtomicU8, Ordering};

pub use pdb_protocol::greeting::{SERIAL_MAX, UNKNOWN_SERIAL as UNKNOWN};

/// Raw chip IDs [`Serial::from_id_bytes`] accepts: 16 bytes is 32 hex
/// characters, the field's cap.
pub const ID_BYTES_MAX: usize = SERIAL_MAX / 2;

/// A validated serial: 1 to [`SERIAL_MAX`] characters of
/// `[0-9A-Za-z._:-]` — safe in a USB string, a shell word and a `pdb -s`
/// argument.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Serial {
    buf: [u8; SERIAL_MAX],
    len: u8,
}

impl Serial {
    /// Uppercase hex of a raw chip ID, most significant byte first as given.
    /// `None` for an empty or oversized ID, and for one that is all `0x00`
    /// or all `0xFF` — an unprogrammed part, or a flash that ignored the
    /// command — so the caller falls back rather than every such board
    /// sharing one serial.
    pub fn from_id_bytes(id: &[u8]) -> Option<Serial> {
        if id.is_empty() || id.len() > ID_BYTES_MAX {
            return None;
        }
        if id.iter().all(|&b| b == 0x00) || id.iter().all(|&b| b == 0xFF) {
            return None;
        }
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut s = Serial {
            buf: [0; SERIAL_MAX],
            len: (id.len() * 2) as u8,
        };
        for (i, &b) in id.iter().enumerate() {
            s.buf[2 * i] = HEX[usize::from(b >> 4)];
            s.buf[2 * i + 1] = HEX[usize::from(b & 0x0F)];
        }
        Some(s)
    }

    /// A family-formatted serial, for a vendor bootloader that shows
    /// something other than plain hex. `None` if it is empty, longer than
    /// [`SERIAL_MAX`], or has a character outside `[0-9A-Za-z._:-]`.
    pub fn from_ascii(s: &str) -> Option<Serial> {
        let bytes = s.as_bytes();
        if bytes.is_empty() || bytes.len() > SERIAL_MAX || !bytes.iter().all(|&b| allowed(b)) {
            return None;
        }
        let mut out = Serial {
            buf: [0; SERIAL_MAX],
            len: bytes.len() as u8,
        };
        out.buf[..bytes.len()].copy_from_slice(bytes);
        Some(out)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..usize::from(self.len)]
    }

    pub fn as_str(&self) -> &str {
        // Every constructor admits ASCII only.
        core::str::from_utf8(self.as_bytes()).unwrap_or(UNKNOWN)
    }

    /// Whether this is a real serial rather than [`UNKNOWN`].
    pub fn is_known(&self) -> bool {
        self.as_bytes() != UNKNOWN.as_bytes()
    }

    fn unknown() -> Serial {
        // The constant is in the allowed set; a test pins that.
        Serial::from_ascii(UNKNOWN).unwrap_or(Serial {
            buf: [b'?'; SERIAL_MAX],
            len: 1,
        })
    }
}

impl core::fmt::Debug for Serial {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

fn allowed(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-')
}

// ── The published value ─────────────────────────────────────────────────

// Written once by `init`, before anything that reads it runs; atomics so the
// readers on other tasks and cores need no lock and no `unsafe`.
static PUBLISHED: [AtomicU8; SERIAL_MAX] = [const { AtomicU8::new(0) }; SERIAL_MAX];
static PUBLISHED_LEN: AtomicU8 = AtomicU8::new(0);

fn publish(s: &Serial) {
    for (cell, &b) in PUBLISHED.iter().zip(s.as_bytes()) {
        cell.store(b, Ordering::Relaxed);
    }
    PUBLISHED_LEN.store(s.len, Ordering::Release);
}

/// This device's serial; [`UNKNOWN`] if [`init`] has not run.
pub fn serial() -> Serial {
    let len = PUBLISHED_LEN.load(Ordering::Acquire);
    if len == 0 {
        return Serial::unknown();
    }
    let mut s = Serial {
        buf: [0; SERIAL_MAX],
        len,
    };
    for (b, cell) in s.buf.iter_mut().zip(&PUBLISHED[..usize::from(len)]) {
        *b = cell.load(Ordering::Relaxed);
    }
    s
}

/// Work out the serial and publish it. Once per boot, from family boot code
/// (see the module docs for when); a second call keeps the first value.
pub fn init() {
    if PUBLISHED_LEN.load(Ordering::Acquire) != 0 {
        return;
    }
    let s = resolve(crate::host::device_serial(), &mut SystemStore, entropy);
    if !s.is_known() {
        crate::pd_warn!("[identity] no chip ID and no entropy: serial unknown");
    }
    publish(&s);
}

// ── The fallback for a chip without an ID ───────────────────────────────

/// Where a made-up serial is kept between boots.
trait Store {
    fn load(&mut self) -> Option<Serial>;
    fn save(&mut self, s: &Serial) -> bool;
}

/// Random bytes made from entropy are this long: 64 bits, the same width as
/// the RP families' hardware IDs.
const RANDOM_ID_BYTES: usize = 8;

/// The policy, apart from the filesystem and the entropy source: the
/// hardware ID if there is one, else the stored serial, else a new random
/// one (stored for next time), else [`UNKNOWN`].
fn resolve(
    hardware: Option<Serial>,
    store: &mut impl Store,
    mut entropy: impl FnMut(&mut [u8]) -> bool,
) -> Serial {
    if let Some(s) = hardware {
        return s;
    }
    if let Some(s) = store.load() {
        return s;
    }
    let mut id = [0u8; RANDOM_ID_BYTES];
    if !entropy(&mut id) {
        return Serial::unknown();
    }
    let Some(s) = Serial::from_id_bytes(&id) else {
        return Serial::unknown();
    };
    if !store.save(&s) {
        crate::pd_warn!(
            "[identity] could not save {}; it will change next boot",
            s.as_str()
        );
    }
    s
}

/// Fill `out` from the hardware entropy source. Boot runs before the
/// scheduler, so this cannot sleep between harvests; it spins, capped,
/// and gives up on a chip that never answers.
fn entropy(out: &mut [u8]) -> bool {
    let mut filled = 0usize;
    crate::spin_until!(
        {
            filled += crate::hal::entropy::fill(&mut out[filled..]);
            filled == out.len()
        },
        2_000_000,
        "serial entropy"
    )
    .is_ok()
}

/// `/system/serial`: the serial as ASCII, written to a temporary file and
/// renamed over the store, as `power.rs` keeps its timeout.
struct SystemStore;

#[cfg(not(test))]
const STORE_DIR: &str = "/system";
#[cfg(not(test))]
const STORE_PATH: &str = "/system/serial";
#[cfg(not(test))]
const STORE_TMP: &str = "/system/serial.tmp";

#[cfg(not(test))]
impl Store for SystemStore {
    fn load(&mut self) -> Option<Serial> {
        use crate::hal::fs;
        if !fs::is_file(STORE_PATH) {
            return None;
        }
        let mut buf = alloc::vec::Vec::new();
        if fs::read_at(STORE_PATH, 0, &mut buf, SERIAL_MAX) < 0 {
            return None;
        }
        Serial::from_ascii(core::str::from_utf8(&buf).ok()?.trim())
    }

    fn save(&mut self, s: &Serial) -> bool {
        use crate::hal::fs;
        if !fs::is_dir(STORE_DIR) && !fs::mkdir(STORE_DIR) {
            return false;
        }
        fs::truncate(STORE_TMP);
        if fs::write_at(STORE_TMP, 0, s.as_bytes()) < 0 {
            let _ = fs::delete(STORE_TMP);
            return false;
        }
        fs::rename(STORE_TMP, STORE_PATH)
    }
}

// The test binary's `TestHooks` always supplies an ID, so `init` never
// reaches the store; `resolve` is tested against a fake below.
#[cfg(test)]
impl Store for SystemStore {
    fn load(&mut self) -> Option<Serial> {
        None
    }
    fn save(&mut self, _s: &Serial) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_uppercase_and_in_byte_order() {
        let s = Serial::from_id_bytes(&[0xE6, 0x63, 0x38, 0x61, 0xA3, 0x7B, 0x90, 0x38]).unwrap();
        assert_eq!(s.as_str(), "E6633861A37B9038");
        // An STM32-sized 96-bit ID fits too.
        let s = Serial::from_id_bytes(&[0x0A; 12]).unwrap();
        assert_eq!(s.as_str().len(), 24);
    }

    #[test]
    fn blank_empty_and_oversized_ids_are_refused() {
        assert_eq!(Serial::from_id_bytes(&[0x00; 8]), None);
        assert_eq!(Serial::from_id_bytes(&[0xFF; 8]), None);
        assert_eq!(Serial::from_id_bytes(&[]), None);
        assert_eq!(Serial::from_id_bytes(&[0x12; ID_BYTES_MAX + 1]), None);
        assert!(Serial::from_id_bytes(&[0x12; ID_BYTES_MAX]).is_some());
    }

    #[test]
    fn ascii_serials_keep_to_the_safe_set() {
        assert_eq!(
            Serial::from_ascii("emulator-4242").unwrap().as_str(),
            "emulator-4242"
        );
        assert!(Serial::from_ascii("AA:BB:CC:DD:EE:FF").is_some());
        assert!(Serial::from_ascii("206A36705430").is_some());
        assert_eq!(Serial::from_ascii(""), None);
        assert_eq!(Serial::from_ascii("has space"), None);
        assert_eq!(Serial::from_ascii("tab\t"), None);
        assert_eq!(Serial::from_ascii("ünï"), None);
        assert_eq!(Serial::from_ascii("a/b"), None);
        let max = "x".repeat(SERIAL_MAX);
        assert!(Serial::from_ascii(&max).is_some());
        assert_eq!(Serial::from_ascii(&(max + "x")), None);
    }

    #[test]
    fn unknown_is_a_valid_serial_that_says_it_is_unknown() {
        let u = Serial::unknown();
        assert_eq!(u.as_str(), UNKNOWN);
        assert!(!u.is_known());
        assert!(Serial::from_ascii(UNKNOWN).is_some());
    }

    #[derive(Default)]
    struct FakeStore {
        stored: Option<Serial>,
        saves: usize,
        save_ok: bool,
    }

    impl Store for FakeStore {
        fn load(&mut self) -> Option<Serial> {
            self.stored
        }
        fn save(&mut self, s: &Serial) -> bool {
            self.saves += 1;
            if self.save_ok {
                self.stored = Some(*s);
            }
            self.save_ok
        }
    }

    fn fixed_entropy(out: &mut [u8]) -> bool {
        for (i, b) in out.iter_mut().enumerate() {
            *b = 0x10 + i as u8;
        }
        true
    }

    fn no_entropy(_: &mut [u8]) -> bool {
        false
    }

    #[test]
    fn a_hardware_id_wins_and_touches_no_store() {
        let hw = Serial::from_ascii("HW").unwrap();
        let mut store = FakeStore {
            stored: Serial::from_ascii("OLD"),
            ..Default::default()
        };
        assert_eq!(resolve(Some(hw), &mut store, fixed_entropy), hw);
        assert_eq!(store.saves, 0);
    }

    #[test]
    fn without_one_the_stored_serial_is_reused() {
        let old = Serial::from_ascii("OLD").unwrap();
        let mut store = FakeStore {
            stored: Some(old),
            ..Default::default()
        };
        assert_eq!(resolve(None, &mut store, no_entropy), old);
        assert_eq!(store.saves, 0);
    }

    #[test]
    fn a_first_boot_makes_a_random_serial_and_keeps_it() {
        let mut store = FakeStore {
            save_ok: true,
            ..Default::default()
        };
        let first = resolve(None, &mut store, fixed_entropy);
        assert_eq!(first.as_str(), "1011121314151617");
        assert_eq!(store.saves, 1);
        // The next boot reads it back rather than drawing again.
        assert_eq!(resolve(None, &mut store, no_entropy), first);
        assert_eq!(store.saves, 1);
    }

    #[test]
    fn no_id_no_store_no_entropy_is_unknown() {
        let mut store = FakeStore::default();
        assert!(!resolve(None, &mut store, no_entropy).is_known());
        // Entropy that came back blank is no better than none.
        let mut store = FakeStore::default();
        let blank = |out: &mut [u8]| {
            out.fill(0);
            true
        };
        assert!(!resolve(None, &mut store, blank).is_known());
    }

    /// `init` publishes what the platform hook says — `TestHooks` here —
    /// and `serial` reads it back on any thread.
    #[test]
    fn init_publishes_the_hooks_serial() {
        init();
        assert_eq!(serial(), crate::host::device_serial().unwrap());
        let s = std::thread::spawn(serial).join().unwrap();
        assert_eq!(s, crate::host::device_serial().unwrap());
    }
}
