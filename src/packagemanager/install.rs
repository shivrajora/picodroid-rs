// SPDX-License-Identifier: GPL-3.0-only
use core::cell::UnsafeCell;

use super::transport::{InstallError, InstallTransport, ReadError};

/// CRC tag byte for install frames.  Included in the CRC computation so the
/// host and device agree on the integrity check regardless of transport.
const CRC_TAG_INSTALL: u8 = 0x01;

/// Bytes pre-buffered from the wire to inspect the PAPK header + manifest
/// before deciding whether to erase flash. Must match the host's
/// [`crate::pdb::protocol::INSTALL_PEEK_BYTES`] — the host inlines exactly
/// this many bytes (or `papk_len`, whichever is smaller) right after the
/// install header so the device can peek without the wire stalling.
const PEEK_BUF_LEN: usize = crate::pdb::protocol::INSTALL_PEEK_BYTES;

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
    papk_len: u32,
) {
    if papk_len == 0 {
        transport.report_error(InstallError::EmptyPayload);
        return;
    }
    if papk_len as usize > super::flash::PAPK_MAX_DATA_SIZE {
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
    let papk_fmv = extract_framework_map_version(&peek_buf[..peeked]);
    if compat::check(papk_fmv, crate::app::FRAMEWORK_MAP_VERSION).is_err() {
        transport.report_error(InstallError::Incompat);
        coordinator.release();
        coordinator.cancel_park_request();
        return;
    }

    // Core 0 is now parked in RAM with interrupts disabled.
    // Safe to erase flash.  Only erase sectors needed for this papk.
    unsafe { super::flash::flash_erase_papk_region(papk_len as usize) };
    transport.report_ready();

    // ── Phase B: stream PAPK bytes, write pages ──────────────────────────
    let mut prefixed = PrefixedTransport {
        prefix: &peek_buf[..peeked],
        pos: 0,
        inner: transport,
    };
    if !stream_and_verify(&mut prefixed, coordinator, papk_len) {
        return;
    }

    // ── Commit metadata, respond, and reboot ─────────────────────────────
    unsafe { super::flash::flash_commit_metadata(papk_len) };
    // Core 0 is still parked in its RAM spin loop.  Report success first so
    // the host sees completion, then reset both cores.
    transport.report_success();
    super::flash::flash_trigger_reset();
}

/// Stream PAPK bytes from the transport, write flash pages, and verify the CRC.
/// Returns `true` on success, `false` if any step fails (error response already
/// sent and core released).
fn stream_and_verify(
    transport: &mut impl InstallTransport,
    coordinator: &mut impl CoreCoordinator,
    len: u32,
) -> bool {
    let mut crc_hasher = crc32fast::Hasher::new();
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
        if !unsafe { super::flash::flash_write_page(page_index, page) } {
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

/// Parse the `framework-map-version` value out of a buffered PAPK prelude.
/// Returns `None` if the header is malformed, the manifest section is
/// truncated within the buffer, or the key is absent.
///
/// Matches the format documented in [`pico_jvm::apk`] — file header at
/// offset 0, manifest section header at the offset stored at file offset
/// 12, manifest section data is a sequence of u16-length-prefixed key/value
/// pairs.
fn extract_framework_map_version(data: &[u8]) -> Option<&str> {
    const HEADER_LEN: usize = 24;
    const SECTION_HEADER_LEN: usize = 16;
    if data.len() < HEADER_LEN || &data[0..4] != b"PAPK" {
        return None;
    }
    let mani_off = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;
    let mani_data_start = mani_off.checked_add(SECTION_HEADER_LEN)?;
    if mani_data_start > data.len() || mani_off + 4 > data.len() {
        return None;
    }
    if &data[mani_off..mani_off + 4] != b"MANI" {
        return None;
    }
    let mani_len = u32::from_le_bytes([
        data[mani_off + 4],
        data[mani_off + 5],
        data[mani_off + 6],
        data[mani_off + 7],
    ]) as usize;
    let mani_data_end = mani_data_start.checked_add(mani_len)?;
    // It's OK if mani_data_end > data.len() — the key may simply be in the
    // unscanned tail. Walk only what we've buffered.
    let scan_end = mani_data_end.min(data.len());

    let mut p = mani_data_start;
    while p + 2 <= scan_end {
        let klen = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
        p += 2;
        if p + klen > scan_end {
            return None;
        }
        let key = &data[p..p + klen];
        p += klen;
        if p + 2 > scan_end {
            return None;
        }
        let vlen = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
        p += 2;
        if p + vlen > scan_end {
            return None;
        }
        let val = &data[p..p + vlen];
        p += vlen;
        if key == b"framework-map-version" {
            return core::str::from_utf8(val).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::extract_framework_map_version;

    /// Build a synthetic PAPK prelude with `entries` as manifest k/v pairs.
    fn build_prelude(entries: &[(&str, &str)]) -> alloc::vec::Vec<u8> {
        let mut manifest = alloc::vec::Vec::new();
        for (k, v) in entries {
            manifest.extend_from_slice(&(k.len() as u16).to_le_bytes());
            manifest.extend_from_slice(k.as_bytes());
            manifest.extend_from_slice(&(v.len() as u16).to_le_bytes());
            manifest.extend_from_slice(v.as_bytes());
        }
        let mani_off: u32 = 24;
        let mut out = alloc::vec::Vec::new();
        out.extend_from_slice(b"PAPK");
        out.extend_from_slice(&1u16.to_le_bytes()); // major
        out.extend_from_slice(&1u16.to_le_bytes()); // minor
        out.extend_from_slice(&2u32.to_le_bytes()); // sec count
        out.extend_from_slice(&mani_off.to_le_bytes());
        out.extend_from_slice(&(mani_off + 16 + manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
                                                    // MANIFEST section header
        out.extend_from_slice(b"MANI");
        out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // crc
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        out.extend_from_slice(&manifest);
        out
    }

    extern crate alloc;

    #[test]
    fn extracts_present_key() {
        let buf = build_prelude(&[("main-class", "x/Y"), ("framework-map-version", "0.1.0")]);
        assert_eq!(extract_framework_map_version(&buf), Some("0.1.0"));
    }

    #[test]
    fn returns_none_when_absent() {
        let buf = build_prelude(&[("main-class", "x/Y")]);
        assert_eq!(extract_framework_map_version(&buf), None);
    }

    #[test]
    fn returns_none_for_bad_magic() {
        let mut buf = build_prelude(&[("framework-map-version", "0.1.0")]);
        buf[0] = 0xFF;
        assert_eq!(extract_framework_map_version(&buf), None);
    }

    #[test]
    fn returns_none_for_truncated_manifest() {
        // Truncate so the manifest section header itself isn't fully present.
        let buf = build_prelude(&[("framework-map-version", "0.1.0")]);
        assert_eq!(extract_framework_map_version(&buf[..28]), None);
    }
}
