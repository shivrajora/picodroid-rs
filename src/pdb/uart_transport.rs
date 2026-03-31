use core::sync::atomic::Ordering;

use freertos_rust::{CurrentTask, Duration};

use super::pending;
use super::protocol::{
    FRAME_MAGIC, STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK, STATUS_READY, STATUS_TOO_LARGE,
};
use crate::packagemanager::install::CoreCoordinator;
use crate::packagemanager::transport::{InstallError, InstallTransport, ReadError};

/// PDBP-over-UART1 transport for PAPK install.
///
/// Reads bytes from the UART1 RX queue (fed by the ISR in `hal::pdb_uart`) and sends
/// PDBP-framed responses via HAL UART write.  Zero-sized — all state lives in
/// the static RX queue owned by `hal::pdb_uart`.
pub struct UartTransport;

impl UartTransport {
    /// Send a PDBP response frame: `[PDBP][status][len LE][payload]`.
    pub(super) fn send_pdbp_response(status: u8, payload: &[u8]) {
        let len = payload.len() as u32;
        for b in FRAME_MAGIC {
            crate::hal::uart::write_byte(1, *b);
        }
        crate::hal::uart::write_byte(1, status);
        for b in len.to_le_bytes() {
            crate::hal::uart::write_byte(1, b);
        }
        for b in payload {
            crate::hal::uart::write_byte(1, *b);
        }
    }
}

impl InstallTransport for UartTransport {
    fn read_byte(&mut self) -> Result<u8, ReadError> {
        crate::hal::pdb_uart::queue_read_byte_timeout().ok_or(ReadError::Timeout)
    }

    fn report_ready(&mut self) {
        Self::send_pdbp_response(STATUS_READY, b"");
    }

    fn report_success(&mut self) {
        Self::send_pdbp_response(STATUS_OK, b"");
        crate::hal::pdb_uart::drain_tx();
    }

    fn report_error(&mut self, error: InstallError) {
        let (status, msg): (u8, &[u8]) = match error {
            InstallError::EmptyPayload => (STATUS_ERR, b""),
            InstallError::TooLarge => (STATUS_TOO_LARGE, b""),
            InstallError::ParkTimeout => (STATUS_ERR, b"park timeout"),
            InstallError::StreamTimeout => (STATUS_ERR, b"stream timeout"),
            InstallError::FlashWriteFailed => (STATUS_ERR, b"flash write failed"),
            InstallError::CrcMismatch => (STATUS_CRC_FAIL, b""),
        };
        Self::send_pdbp_response(status, msg);
    }
}

// ── PDB core coordinator ────────────────────────────────────────────────────

/// Coordinates JVM stop and core-0 parking using the PDB pending flags.
pub(super) struct PdbCoreCoordinator;

impl CoreCoordinator for PdbCoreCoordinator {
    fn request_stop_and_park(&mut self) {
        // Clear stale flags from any previous failed install attempt so
        // core 0 does not see a leftover CORE0_RELEASE and exit the park
        // loop immediately.
        pending::CORE0_RELEASE.store(false, Ordering::Relaxed);
        pending::CORE0_PARKED.store(false, Ordering::Relaxed);

        pending::STOP_JVM.store(true, Ordering::Release);
        pending::FLASH_PARK_REQUESTED.store(true, Ordering::Release);
        pending::notify_jvm();
    }

    fn wait_for_park(&mut self) -> bool {
        for _ in 0..1500 {
            if pending::CORE0_PARKED.load(Ordering::Acquire) {
                return true;
            }
            CurrentTask::delay(Duration::ms(10));
        }
        false
    }

    fn release(&mut self) {
        pending::CORE0_RELEASE.store(true, Ordering::Release);
    }

    fn cancel_park_request(&mut self) {
        pending::FLASH_PARK_REQUESTED.store(false, Ordering::Release);
    }
}
