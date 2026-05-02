// SPDX-License-Identifier: GPL-3.0-only
use core::sync::atomic::Ordering;

use freertos_rust::{CurrentTask, Duration};

use super::pending;
use super::protocol::{
    FRAME_MAGIC, STATUS_CRC_FAIL, STATUS_ERR, STATUS_INCOMPAT, STATUS_OK, STATUS_READY,
    STATUS_TOO_LARGE,
};
use crate::packagemanager::install::CoreCoordinator;
use crate::packagemanager::transport::{InstallError, InstallTransport, ReadError};

/// PDBP-over-USB-CDC transport for PAPK install.
///
/// Reads bytes from the USB CDC RX queue (fed by the ISR in `hal::pdb_usb`) and
/// sends PDBP-framed responses via bulk IN endpoint.  Zero-sized — all state
/// lives in the static RX queue owned by `hal::pdb_usb`.
pub struct CdcTransport;

impl CdcTransport {
    /// Send a PDBP response frame: `[PDBP][status][len LE][payload]`.
    pub(super) fn send_pdbp_response(status: u8, payload: &[u8]) {
        let len = payload.len() as u32;
        let hdr_len = FRAME_MAGIC.len() + 1 + 4; // magic + status + len
        let total = hdr_len + payload.len();
        // Build frame in a stack buffer and send as one bulk write.
        let mut buf = [0u8; 9 + 256]; // 9-byte header + max typical payload
        buf[..4].copy_from_slice(FRAME_MAGIC);
        buf[4] = status;
        buf[5..9].copy_from_slice(&len.to_le_bytes());
        let copy_len = payload.len().min(buf.len() - 9);
        buf[9..9 + copy_len].copy_from_slice(&payload[..copy_len]);
        crate::hal::pdb_usb::write_bytes(&buf[..total.min(buf.len())]);
        // If payload exceeds stack buffer (unlikely), send remainder directly.
        if payload.len() > copy_len {
            crate::hal::pdb_usb::write_bytes(&payload[copy_len..]);
        }
    }
}

impl InstallTransport for CdcTransport {
    fn read_byte(&mut self) -> Result<u8, ReadError> {
        // On RP2350 (configTICK_CORE=0), core 0 is parked during install so
        // the FreeRTOS tick is frozen.  Use non-blocking queue poll + hardware
        // timer instead of tick-based timeouts.
        #[cfg(feature = "chip-rp2350")]
        return crate::hal::pdb_usb::queue_read_byte_busywait(2_000_000).ok_or(ReadError::Timeout);
        #[cfg(not(feature = "chip-rp2350"))]
        crate::hal::pdb_usb::queue_read_byte_timeout().ok_or(ReadError::Timeout)
    }

    fn report_ready(&mut self) {
        Self::send_pdbp_response(STATUS_READY, b"");
    }

    fn report_success(&mut self) {
        Self::send_pdbp_response(STATUS_OK, b"");
        crate::hal::pdb_usb::drain_tx();
    }

    fn report_error(&mut self, error: InstallError) {
        let (status, msg): (u8, &[u8]) = match error {
            InstallError::EmptyPayload => (STATUS_ERR, b""),
            InstallError::TooLarge => (STATUS_TOO_LARGE, b""),
            InstallError::ParkTimeout => (STATUS_ERR, b"park timeout"),
            InstallError::StreamTimeout => (STATUS_ERR, b"stream timeout"),
            InstallError::FlashWriteFailed => (STATUS_ERR, b"flash write failed"),
            InstallError::CrcMismatch => (STATUS_CRC_FAIL, b""),
            InstallError::Incompat => (STATUS_INCOMPAT, b"framework-map-version mismatch"),
        };
        Self::send_pdbp_response(status, msg);
    }
}

// ── PDB core coordinator ────────────────────────────────────────────────────

/// Coordinates JVM stop and core-0 parking using the PDB pending flags.
pub(super) struct PdbCoreCoordinator;

impl CoreCoordinator for PdbCoreCoordinator {
    fn request_stop_and_park(&mut self) {
        pending::CORE0_PARKED.store(false, Ordering::Relaxed);

        pending::set_stop_jvm();
        // Relaxed is fine: PDB and JVM share core 0, so the store is
        // immediately visible without a cross-core barrier.
        pending::FLASH_PARK_REQUESTED.store(true, Ordering::Relaxed);
        // We intentionally do NOT call abort_jvm_delay() here.  PDB and
        // JVM share core 0, so PDB (higher priority) must yield via
        // notify_jvm() to let the JVM task run and observe STOP_JVM.
        pending::notify_jvm();
    }

    fn wait_for_park(&mut self) -> bool {
        // PDB and JVM share core 0.  PDB (higher priority) must yield
        // so the JVM task can run and exit.
        for _ in 0..1500u32 {
            if pending::CORE0_PARKED.load(Ordering::Acquire) {
                return true;
            }
            CurrentTask::delay(Duration::ms(10));
        }
        false
    }

    fn release(&mut self) {
        pending::CORE0_PARKED.store(false, Ordering::Relaxed);
        pending::notify_jvm();
    }

    fn cancel_park_request(&mut self) {
        pending::FLASH_PARK_REQUESTED.store(false, Ordering::Release);
    }
}
