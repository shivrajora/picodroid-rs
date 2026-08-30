// SPDX-License-Identifier: GPL-3.0-only
use core::cell::UnsafeCell;

use super::transport::{InstallError, InstallTransport, ReadError};

/// CRC tag byte for install frames. Included in the CRC computation so the
/// host and device agree on the integrity check regardless of transport.
///
/// Aliased rather than restated: the host seeds its hasher with `CMD_INSTALL`
/// (`pdb_protocol::crc32_frame`), so the two are not merely equal by
/// convention — a divergence would fail every install with `STATUS_CRC_FAIL`.
/// The alias keeps this module reading transport-agnostically while the value
/// stays single-sourced.
const CRC_TAG_INSTALL: u8 = pdb_protocol::CMD_INSTALL;

/// Bytes pre-buffered from the wire to inspect the PAPK header + manifest
/// before deciding whether to erase flash. The host inlines exactly this many
/// bytes (or `papk_len`, whichever is smaller) right after the install header
/// so the device can peek without the wire stalling — both ends read the
/// count from `pdb-protocol`, so they cannot disagree.
const PEEK_BUF_LEN: usize = pdb_protocol::INSTALL_PEEK_BYTES;

// ── 256-byte page buffer for streaming flash writes ───────────────────────────
//
// Used only during install handling.
// SAFETY: single-core; only install logic writes, no concurrent reader.
struct PageBufCell(UnsafeCell<[u8; 256]>);
unsafe impl Sync for PageBufCell {}
static PAGE_BUF: PageBufCell = PageBufCell(UnsafeCell::new([0u8; 256]));

/// Coordinates with the JVM core during flash operations.
///
/// Implementors manage the mechanics of stopping the JVM, parking the
/// execution core, and releasing it after flash operations complete.
/// This trait decouples the install orchestrator from any specific
/// inter-core synchronization mechanism.
pub trait CoreCoordinator {
    /// Signal the JVM to stop and request the execution core to park for flash.
    fn request_stop_and_park(&mut self);

    /// Poll until the execution core has parked.  Returns `false` on timeout.
    fn wait_for_park(&mut self) -> bool;

    /// Release the execution core from its RAM spin loop.
    fn release(&mut self);

    /// Cancel the park request (cleanup after a park timeout).
    fn cancel_park_request(&mut self);
}

/// The family's PAPK flash slot.
///
/// The last free-function seam in the install path: `CoreCoordinator` and
/// [`InstallTransport`] were already traits, while flash was reached through
/// `super::flash::*`, which is what kept this module in the family crate and
/// out of every test binary.
///
/// # Safety
///
/// Implementors must uphold what the orchestrator cannot check: `erase_region`
/// and `write_page` may run only while the JVM core is parked, because on a
/// family that executes in place from this same flash, erasing it under a
/// running core faults. `run_install` guarantees the park; an implementor that
/// takes these calls from anywhere else does not inherit that guarantee.
pub unsafe trait PapkFlash {
    /// Largest PAPK this slot can hold, in bytes.
    fn max_data_size(&self) -> usize;

    /// Erase enough of the slot to hold `papk_len` bytes, plus the metadata
    /// sector.
    ///
    /// # Safety
    /// The JVM core must be parked. See the trait docs.
    unsafe fn erase_region(&mut self, papk_len: usize);

    /// Program one 256-byte page at `page_index` into the data region.
    /// Returns false if the page falls outside the slot.
    ///
    /// # Safety
    /// The JVM core must be parked. See the trait docs.
    unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool;

    /// Write the boot-meta header, committing the install atomically. Until
    /// this lands, a partially written slot still reads as the previous app —
    /// or as no app, which is why it is the last write.
    ///
    /// # Safety
    /// The JVM core must be parked. See the trait docs.
    unsafe fn commit_metadata(&mut self, len: u32);

    /// Reboot into the newly installed app. Never returns.
    fn trigger_reset(&mut self) -> !;
}

// ── Transport-agnostic install orchestration ─────────────────────────────────

/// Run the full PAPK install sequence over the given transport.
///
/// Phases:
///   A. Validate size, stop JVM, park core 0, erase flash
///   B. Stream PAPK bytes page-by-page, computing CRC incrementally
///   C. Verify CRC, commit metadata, report success, trigger reset
///
/// On any error, the execution core is released and the error is reported
/// via the transport.  The caller can then resume its command loop.
pub fn run_install(
    transport: &mut impl InstallTransport,
    coordinator: &mut impl CoreCoordinator,
    flash: &mut impl PapkFlash,
    papk_len: u32,
) {
    if papk_len == 0 {
        transport.report_error(InstallError::EmptyPayload);
        return;
    }
    if papk_len as usize > flash.max_data_size() {
        transport.report_error(InstallError::TooLarge);
        return;
    }

    // ── Phase A: stop JVM, park core 0, erase flash ──────────────────────
    coordinator.request_stop_and_park();

    if !coordinator.wait_for_park() {
        transport.report_error(InstallError::ParkTimeout);
        coordinator.release();
        coordinator.cancel_park_request();
        return;
    }

    // ── Pre-erase compat check ────────────────────────────────────────────
    // Peek the PAPK file header + manifest section off the wire (without
    // erasing flash yet) and verify the framework-map-version matches the
    // firmware's. A mismatched PAPK is rejected here, leaving the existing
    // PAPK in flash intact. The buffered bytes are replayed into
    // stream_and_verify via PrefixedTransport so they're still hashed and
    // written to flash in Phase B.
    let mut peek_buf = [0u8; PEEK_BUF_LEN];
    let peeked = (papk_len as usize).min(PEEK_BUF_LEN);
    for b in peek_buf[..peeked].iter_mut() {
        match transport.read_byte() {
            Ok(byte) => *b = byte,
            Err(_) => {
                transport.report_error(InstallError::StreamTimeout);
                coordinator.release();
                coordinator.cancel_park_request();
                return;
            }
        }
    }
    let papk_fmv = papk_format::find_manifest_value_in_prefix(
        &peek_buf[..peeked],
        papk_format::keys::FRAMEWORK_MAP_VERSION,
    );
    if compat::check(papk_fmv, crate::framework_map::FRAMEWORK_MAP_VERSION).is_err() {
        transport.report_error(InstallError::Incompat);
        coordinator.release();
        coordinator.cancel_park_request();
        return;
    }

    // Core 0 is now parked in RAM with interrupts disabled.
    // Safe to erase flash.  Only erase sectors needed for this papk.
    unsafe { flash.erase_region(papk_len as usize) };
    transport.report_ready();

    // ── Phase B: stream PAPK bytes, write pages ──────────────────────────
    let mut prefixed = PrefixedTransport {
        prefix: &peek_buf[..peeked],
        pos: 0,
        inner: transport,
    };
    if !stream_and_verify(&mut prefixed, coordinator, flash, papk_len) {
        // stream_and_verify released core 0; withdraw the park request as
        // the Phase-A failure paths do, otherwise jvm_task re-parks on its
        // next supervisor loop and the device is wedged until another
        // install arrives.
        coordinator.cancel_park_request();
        return;
    }

    // ── Commit metadata, respond, and reboot ─────────────────────────────
    unsafe { flash.commit_metadata(papk_len) };
    // Core 0 is still parked in its RAM spin loop.  Report success first so
    // the host sees completion, then reset both cores.
    transport.report_success();
    flash.trigger_reset()
}

/// Stream PAPK bytes from the transport, write flash pages, and verify the CRC.
/// Returns `true` on success, `false` if any step fails (error response already
/// sent and core released).
fn stream_and_verify(
    transport: &mut impl InstallTransport,
    coordinator: &mut impl CoreCoordinator,
    flash: &mut impl PapkFlash,
    len: u32,
) -> bool {
    let mut crc_hasher = pdb_protocol::Crc32::new();
    crc_hasher.update(&[CRC_TAG_INSTALL]);
    crc_hasher.update(&len.to_le_bytes());

    let mut bytes_remaining = len as usize;
    let mut page_index: u32 = 0;
    while bytes_remaining > 0 {
        let chunk = bytes_remaining.min(256);
        let page = unsafe { &mut *PAGE_BUF.0.get() };
        for b in page[..chunk].iter_mut() {
            match transport.read_byte() {
                Ok(byte) => *b = byte,
                Err(_) => {
                    coordinator.release();
                    transport.report_error(InstallError::StreamTimeout);
                    return false;
                }
            }
        }
        crc_hasher.update(&page[..chunk]);

        if chunk < 256 {
            page[chunk..].fill(0xFF);
        }
        if !unsafe { flash.write_page(page_index, page) } {
            coordinator.release();
            transport.report_error(InstallError::FlashWriteFailed);
            return false;
        }
        page_index += 1;
        bytes_remaining -= chunk;
    }

    let wire_crc = match transport.read_u32_le() {
        Ok(v) => v,
        Err(_) => {
            coordinator.release();
            transport.report_error(InstallError::StreamTimeout);
            return false;
        }
    };
    if crc_hasher.finalize() != wire_crc {
        coordinator.release();
        transport.report_error(InstallError::CrcMismatch);
        return false;
    }
    true
}

/// Adapter that yields a static byte prefix before delegating to the inner
/// transport. Used to replay PAPK bytes pre-buffered by the compat check
/// into [`stream_and_verify`] without changing its signature.
struct PrefixedTransport<'a, T: InstallTransport> {
    prefix: &'a [u8],
    pos: usize,
    inner: &'a mut T,
}

impl<T: InstallTransport> InstallTransport for PrefixedTransport<'_, T> {
    fn read_byte(&mut self) -> Result<u8, ReadError> {
        if self.pos < self.prefix.len() {
            let b = self.prefix[self.pos];
            self.pos += 1;
            Ok(b)
        } else {
            self.inner.read_byte()
        }
    }
    fn report_ready(&mut self) {
        self.inner.report_ready();
    }
    fn report_success(&mut self) {
        self.inner.report_success();
    }
    fn report_error(&mut self, error: InstallError) {
        self.inner.report_error(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// `PAGE_BUF` is a process-wide static — deliberately, so a 256-byte
    /// staging buffer does not sit on the debug bridge's stack. On device the
    /// single-core install path is its only writer; in a test binary the
    /// harness runs tests on parallel threads, so they take this lock. The
    /// alternative, making the buffer a local, would change what runs on the
    /// device to suit the tests.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// What the orchestrator did, in order. Ordering is the property most of
    /// these tests exist to pin: an erase that happens before a rejection has
    /// already destroyed the installed app.
    #[derive(Debug, PartialEq, Eq)]
    enum Ev {
        RequestPark,
        Release,
        CancelPark,
        Erase(usize),
        Write(u32),
        Commit(u32),
        Ready,
        Success,
        Error(&'static str),
    }

    fn err_name(e: &InstallError) -> &'static str {
        match e {
            InstallError::EmptyPayload => "empty",
            InstallError::TooLarge => "too_large",
            InstallError::ParkTimeout => "park_timeout",
            InstallError::StreamTimeout => "stream_timeout",
            InstallError::FlashWriteFailed => "flash_write_failed",
            InstallError::CrcMismatch => "crc_mismatch",
            InstallError::Incompat => "incompat",
        }
    }

    /// Shared recorder. `Rc`-free: the three mocks each hold a raw pointer to
    /// one `Vec` owned by the test, which is sound because they never outlive
    /// it and the test is single-threaded under `SERIAL`.
    struct Log(*mut Vec<Ev>);
    impl Log {
        fn push(&self, e: Ev) {
            unsafe { (*self.0).push(e) }
        }
    }

    struct MockTransport {
        log: Log,
        data: Vec<u8>,
        pos: usize,
        /// Byte offset at which reads start timing out, if any.
        cut_at: Option<usize>,
    }

    impl InstallTransport for MockTransport {
        fn read_byte(&mut self) -> Result<u8, ReadError> {
            if self.cut_at == Some(self.pos) {
                return Err(ReadError::Timeout);
            }
            let b = *self.data.get(self.pos).ok_or(ReadError::Timeout)?;
            self.pos += 1;
            Ok(b)
        }
        fn report_ready(&mut self) {
            self.log.push(Ev::Ready)
        }
        fn report_success(&mut self) {
            self.log.push(Ev::Success)
        }
        fn report_error(&mut self, e: InstallError) {
            self.log.push(Ev::Error(err_name(&e)))
        }
    }

    struct MockCoordinator {
        log: Log,
        parks: bool,
    }

    impl CoreCoordinator for MockCoordinator {
        fn request_stop_and_park(&mut self) {
            self.log.push(Ev::RequestPark)
        }
        fn wait_for_park(&mut self) -> bool {
            self.parks
        }
        fn release(&mut self) {
            self.log.push(Ev::Release)
        }
        fn cancel_park_request(&mut self) {
            self.log.push(Ev::CancelPark)
        }
    }

    struct MockFlash {
        log: Log,
        max: usize,
        /// Page index whose write fails, if any.
        fail_page: Option<u32>,
        /// Everything handed to `write_page`, concatenated.
        written: Vec<u8>,
    }

    unsafe impl PapkFlash for MockFlash {
        fn max_data_size(&self) -> usize {
            self.max
        }
        unsafe fn erase_region(&mut self, papk_len: usize) {
            self.log.push(Ev::Erase(papk_len))
        }
        unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool {
            if self.fail_page == Some(page_index) {
                return false;
            }
            self.log.push(Ev::Write(page_index));
            self.written.extend_from_slice(page);
            true
        }
        unsafe fn commit_metadata(&mut self, len: u32) {
            self.log.push(Ev::Commit(len))
        }
        fn trigger_reset(&mut self) -> ! {
            // A real one reboots. Unwinding out of `run_install` is the
            // closest a test can get to "never returns" while still letting
            // the assertions run; `run_install` does nothing after this call.
            panic!("__reset__")
        }
    }

    fn crc_of(papk: &[u8]) -> u32 {
        let mut h = pdb_protocol::Crc32::new();
        h.update(&[CRC_TAG_INSTALL]);
        h.update(&(papk.len() as u32).to_le_bytes());
        h.update(papk);
        h.finalize()
    }

    /// Wire bytes for a successful install: the PAPK followed by its CRC.
    fn wire(papk: &[u8]) -> Vec<u8> {
        let mut v = papk.to_vec();
        v.extend_from_slice(&crc_of(papk).to_le_bytes());
        v
    }

    struct Run {
        events: Vec<Ev>,
        written: Vec<u8>,
        reset: bool,
    }

    fn run(
        papk_len: u32,
        wire_bytes: Vec<u8>,
        parks: bool,
        max: usize,
        fail_page: Option<u32>,
    ) -> Run {
        let _g = lock();
        let mut events: Vec<Ev> = Vec::new();
        let ptr: *mut Vec<Ev> = &mut events;
        let mut t = MockTransport {
            log: Log(ptr),
            data: wire_bytes,
            pos: 0,
            cut_at: None,
        };
        let mut c = MockCoordinator {
            log: Log(ptr),
            parks,
        };
        let mut f = MockFlash {
            log: Log(ptr),
            max,
            fail_page,
            written: Vec::new(),
        };
        let reset = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_install(&mut t, &mut c, &mut f, papk_len);
        }))
        .is_err();
        Run {
            events,
            written: f.written,
            reset,
        }
    }

    /// The version this test binary's firmware claims. `test.sh` runs the
    /// suite in both shrink modes, so it is the `"0.0.0"` no-shrink sentinel
    /// in one pass and a real version in the other — the tests derive their
    /// PAPK versions from it rather than hardcoding either.
    const FW: &str = crate::framework_map::FRAMEWORK_MAP_VERSION;

    /// A version `compat::check` always refuses against [`FW`], in either
    /// mode. The rule that makes this work is the asymmetric one: exactly one
    /// side being the `"0.0.0"` sentinel means shrunk-vs-unshrunk, which is
    /// rejected regardless of ordering.
    fn incompatible_version() -> &'static str {
        if FW == "0.0.0" {
            "9.9.9"
        } else {
            "0.0.0"
        }
    }

    /// Build a real PAPK declaring `fmv` as its framework-map-version.
    ///
    /// Hand-rolled bytes would drift from the format; this goes through the
    /// same writer `papk-pack` uses, so the manifest the device peeks at is
    /// shaped exactly like a shipped one.
    fn papk_with_fmv(fmv: &str) -> Vec<u8> {
        use papk_format::{EntryPoint, ManifestSpec, PapkBuilder};
        PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("demo/Main"),
            package_name: "demo",
            version: "1.0",
            framework_map_version: fmv,
        })
        .build()
        .expect("test PAPK")
    }

    /// A compatible PAPK padded to exactly `len` bytes.
    ///
    /// The byte-level tests care about page geometry, not about content, but
    /// they still have to get past the compat gate — so they start from a
    /// real manifest and pad. Trailing filler is fine: the gate only scans
    /// the peeked prefix, and the install path does not otherwise parse the
    /// image.
    ///
    /// Panics rather than silently shrinking if `len` is below the minimum a
    /// real PAPK occupies, so a test cannot quietly stop testing what it says.
    fn papk_of_len(len: usize) -> Vec<u8> {
        let mut v = papk_with_fmv(FW);
        assert!(
            len >= v.len(),
            "test asked for a {len}-byte PAPK; the smallest real one is {}",
            v.len()
        );
        let base = v.len();
        v.extend((0..len - base).map(|i| (i % 251) as u8));
        v
    }

    /// Smallest PAPK these tests can build, so sizes can be expressed
    /// relative to it rather than as magic numbers that break when the
    /// manifest changes.
    fn min_papk_len() -> usize {
        papk_with_fmv(FW).len()
    }

    // ── Phase A must not touch flash ─────────────────────────────────────

    /// The compat gate is the one rejection that happens *after* the core is
    /// parked and *before* the erase — the narrowest window in the whole
    /// sequence, and the one that protects a working device from a PAPK built
    /// against different firmware.
    #[test]
    fn an_incompatible_papk_is_refused_with_flash_untouched() {
        let papk = papk_with_fmv(incompatible_version());
        let r = run(papk.len() as u32, wire(&papk), true, 1 << 20, None);

        assert!(
            r.events.contains(&Ev::Error("incompat")),
            "expected an incompat rejection, got {:?}",
            r.events
        );
        assert!(
            !r.events.iter().any(|e| matches!(e, Ev::Erase(_))),
            "an incompatible PAPK erased the installed app: {:?}",
            r.events
        );
        assert!(!r.events.iter().any(|e| matches!(e, Ev::Commit(_))));
        assert!(!r.reset);
        // The core was parked before the check, so it must be let go again.
        assert!(r.events.contains(&Ev::Release));
        assert!(r.events.contains(&Ev::CancelPark));
    }

    /// The matching case: a PAPK built against this firmware's own version
    /// installs. Without it, the test above would pass even if the gate
    /// rejected everything.
    #[test]
    fn a_matching_papk_is_accepted() {
        let papk = papk_with_fmv(FW);
        let r = run(papk.len() as u32, wire(&papk), true, 1 << 20, None);
        assert!(
            !r.events.iter().any(|e| matches!(e, Ev::Error(_))),
            "a compatible PAPK was refused: {:?}",
            r.events
        );
        assert!(r.events.contains(&Ev::Commit(papk.len() as u32)));
    }

    /// The whole point of validating before erasing: a refused install leaves
    /// the app already on the device intact. If any of these ever erased
    /// first, a bad PAPK would brick a working device.
    #[test]
    fn rejected_installs_never_erase() {
        for (name, r) in [
            ("empty", run(0, Vec::new(), true, 1024, None)),
            ("too_large", run(2048, Vec::new(), true, 1024, None)),
            ("park_timeout", run(16, Vec::new(), false, 1 << 20, None)),
        ] {
            assert!(
                !r.events.iter().any(|e| matches!(e, Ev::Erase(_))),
                "{name} erased flash before rejecting"
            );
            assert!(
                !r.events.iter().any(|e| matches!(e, Ev::Commit(_))),
                "{name} committed metadata"
            );
            assert!(!r.reset, "{name} rebooted the device");
        }
    }

    #[test]
    fn empty_and_oversized_are_refused_without_even_parking() {
        let empty = run(0, Vec::new(), true, 1024, None);
        assert_eq!(empty.events, vec![Ev::Error("empty")]);

        let big = run(2048, Vec::new(), true, 1024, None);
        assert_eq!(big.events, vec![Ev::Error("too_large")]);
    }

    /// `max_data_size` is the family's slot ceiling, so exactly-at-the-limit
    /// must be accepted — an off-by-one here silently caps installable apps.
    #[test]
    fn a_papk_exactly_at_the_limit_is_accepted() {
        let n = min_papk_len() + 64;
        let papk = papk_of_len(n);
        let r = run(n as u32, wire(&papk), true, n, None);
        assert!(r.events.contains(&Ev::Erase(n)));
        assert!(r.events.contains(&Ev::Commit(n as u32)));
    }

    #[test]
    fn park_timeout_releases_and_cancels_before_returning() {
        let r = run(16, Vec::new(), false, 1 << 20, None);
        assert_eq!(
            r.events,
            vec![
                Ev::RequestPark,
                Ev::Error("park_timeout"),
                Ev::Release,
                Ev::CancelPark
            ]
        );
    }

    // ── Phase B ──────────────────────────────────────────────────────────

    /// The peek buffer reads bytes off the wire *before* the erase, to run
    /// the compat check. Those bytes must still reach flash — they are the
    /// front of the PAPK. Replaying them is what `PrefixedTransport` is for,
    /// and dropping them would write a subtly corrupt image that still passes
    /// its own CRC only if the CRC were computed post-replay (it is not).
    #[test]
    fn peeked_bytes_are_replayed_into_flash() {
        // Longer than one peek buffer so the path has both a replayed prefix
        // and a streamed tail.
        let papk = papk_of_len(PEEK_BUF_LEN + 300);
        let r = run(papk.len() as u32, wire(&papk), true, 1 << 20, None);
        assert!(r.events.contains(&Ev::Commit(papk.len() as u32)));
        assert_eq!(
            &r.written[..papk.len()],
            &papk[..],
            "flash content diverged from the PAPK that was sent"
        );
    }

    /// A PAPK shorter than the peek buffer takes the path where the peek is
    /// truncated to `papk_len` — the boundary the `.min()` guards.
    #[test]
    fn a_papk_shorter_than_the_peek_buffer_round_trips() {
        let n = min_papk_len();
        assert!(
            n < PEEK_BUF_LEN,
            "minimal PAPK must be under one peek buffer"
        );
        let papk = papk_of_len(n);
        let r = run(n as u32, wire(&papk), true, 1 << 20, None);
        assert_eq!(&r.written[..n], &papk[..]);
        assert!(r.events.contains(&Ev::Commit(n as u32)));
    }

    /// Trailing bytes of the last partial page are padded with the erased
    /// value, not left as whatever the previous install put in the buffer.
    #[test]
    fn final_partial_page_is_padded_with_erased_bytes() {
        // Length chosen to land mid-page so there is a tail to pad.
        let n = 300.max(min_papk_len() + 1);
        let n = if n % 256 == 0 { n + 1 } else { n };
        let papk = papk_of_len(n);
        let r = run(n as u32, wire(&papk), true, 1 << 20, None);
        let pages = n.div_ceil(256);
        assert_eq!(r.written.len(), pages * 256);
        assert!(
            r.written[n..].iter().all(|&b| b == 0xFF),
            "partial page tail was not padded with 0xFF"
        );
    }

    #[test]
    fn crc_mismatch_releases_the_core_and_does_not_commit() {
        let papk = papk_of_len(min_papk_len() + 32);
        let mut bad = papk.clone();
        bad.extend_from_slice(&crc_of(&papk).wrapping_add(1).to_le_bytes());
        let r = run(papk.len() as u32, bad, true, 1 << 20, None);
        assert!(r.events.contains(&Ev::Error("crc_mismatch")));
        assert!(r.events.contains(&Ev::Release));
        // The park request must be withdrawn too, or jvm_task parks itself
        // on its next supervisor loop and the device wedges (bugbash F8).
        assert!(r.events.contains(&Ev::CancelPark));
        assert!(!r.events.iter().any(|e| matches!(e, Ev::Commit(_))));
        assert!(!r.reset);
    }

    #[test]
    fn a_failed_page_write_aborts_without_committing() {
        let papk = papk_of_len(600); // three pages
        let r = run(600, wire(&papk), true, 1 << 20, Some(1));
        assert!(r.events.contains(&Ev::Error("flash_write_failed")));
        assert!(r.events.contains(&Ev::Release));
        // The park request must be withdrawn too, or jvm_task parks itself
        // on its next supervisor loop and the device wedges (bugbash F8).
        assert!(r.events.contains(&Ev::CancelPark));
        assert!(!r.events.iter().any(|e| matches!(e, Ev::Commit(_))));
    }

    // ── Ordering ─────────────────────────────────────────────────────────

    /// Commit is the atomic point: until the boot-meta lands the slot reads
    /// as the old app or as none. It must therefore come after every page,
    /// and success must be reported before the reboot that cuts the link.
    #[test]
    fn commit_follows_every_page_and_precedes_success_then_reset() {
        let papk = papk_of_len(600);
        let r = run(600, wire(&papk), true, 1 << 20, None);

        let pos = |want: &Ev| r.events.iter().position(|e| e == want).unwrap();
        let last_write = r
            .events
            .iter()
            .rposition(|e| matches!(e, Ev::Write(_)))
            .unwrap();

        assert!(pos(&Ev::Erase(600)) < last_write);
        assert!(last_write < pos(&Ev::Commit(600)));
        assert!(pos(&Ev::Commit(600)) < pos(&Ev::Success));
        assert!(
            r.reset,
            "device was not rebooted after a successful install"
        );
    }

    /// Ready is the host's cue to start streaming, so it must not be sent
    /// before flash is actually erased and able to take the data.
    #[test]
    fn ready_is_reported_only_after_the_erase() {
        let papk = papk_of_len(min_papk_len() + 100);
        let r = run(papk.len() as u32, wire(&papk), true, 1 << 20, None);
        let erase = r.events.iter().position(|e| matches!(e, Ev::Erase(_)));
        let ready = r.events.iter().position(|e| *e == Ev::Ready);
        assert!(erase.is_some() && ready.is_some());
        assert!(erase < ready);
    }
}
