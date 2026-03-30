use core::cell::UnsafeCell;

use super::transport::{InstallError, InstallTransport};

/// CRC tag byte for install frames.  Included in the CRC computation so the
/// host and device agree on the integrity check regardless of transport.
const CRC_TAG_INSTALL: u8 = 0x01;

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

    // Core 0 is now parked in RAM with interrupts disabled.
    // Safe to erase flash.  Only erase sectors needed for this papk.
    unsafe { super::flash::flash_erase_papk_region(papk_len as usize) };
    transport.report_ready();

    // ── Phase B: stream PAPK bytes, write pages ──────────────────────────
    if !stream_and_verify(transport, coordinator, papk_len) {
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
