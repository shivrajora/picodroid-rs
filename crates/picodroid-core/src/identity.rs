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

// The value type — what a chip ID or a stored string must look like to be
// a serial — is `pd_codec::serial`, so the host fuzzes it (`sysfmt`).
pub use pd_codec::serial::{Serial, ID_BYTES_MAX, SERIAL_MAX, UNKNOWN};

// ── The published value ─────────────────────────────────────────────────

// Written once by `init`, before anything that reads it runs; atomics so the
// readers on other tasks and cores need no lock and no `unsafe`.
static PUBLISHED: [AtomicU8; SERIAL_MAX] = [const { AtomicU8::new(0) }; SERIAL_MAX];
static PUBLISHED_LEN: AtomicU8 = AtomicU8::new(0);

fn publish(s: &Serial) {
    let bytes = s.as_bytes();
    for (cell, &b) in PUBLISHED.iter().zip(bytes) {
        cell.store(b, Ordering::Relaxed);
    }
    PUBLISHED_LEN.store(bytes.len() as u8, Ordering::Release);
}

/// This device's serial; [`UNKNOWN`] if [`init`] has not run.
pub fn serial() -> Serial {
    let len = usize::from(PUBLISHED_LEN.load(Ordering::Acquire));
    if len == 0 {
        return Serial::unknown();
    }
    let mut buf = [0u8; SERIAL_MAX];
    for (b, cell) in buf.iter_mut().zip(&PUBLISHED[..len]) {
        *b = cell.load(Ordering::Relaxed);
    }
    // What was published passed a constructor once; `publish` is the only
    // writer.
    Serial::from_published(buf, len as u8)
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
