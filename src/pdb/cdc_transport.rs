use core::sync::atomic::Ordering;

#[cfg(not(feature = "chip-rp2350-hal"))]
use freertos_rust::{CurrentTask, Duration};

use super::pending;
use super::protocol::{
    FRAME_MAGIC, STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK, STATUS_READY, STATUS_TOO_LARGE,
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
        #[cfg(feature = "chip-rp2350-hal")]
        return crate::hal::pdb_usb::queue_read_byte_busywait(2_000_000).ok_or(ReadError::Timeout);
        #[cfg(not(feature = "chip-rp2350-hal"))]
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
        pending::abort_jvm_delay();
        pending::notify_jvm();
    }

    fn wait_for_park(&mut self) -> bool {
        // On RP2350 (configTICK_CORE=0), core 0 is about to park which
        // will freeze the FreeRTOS tick.  We cannot use CurrentTask::delay
        // because the tick may freeze mid-delay, hanging core 1.
        //
        // A bare tight loop also does not work: it starves the FreeRTOS
        // SMP scheduler on core 0 so vTaskDelay never completes and the
        // JVM cannot see STOP_JVM.  The workaround is a hardware-timer
        // poll with a 1 ms gap between checks (using NOP, not timer reads)
        // to give the bus/scheduler breathing room.
        // On RP2350 (configTICK_CORE=0), the tick freezes when core 0
        // parks.  A FreeRTOS-managed blocking wait on core 1 lets core 0
        // run (so the JVM can exit), but once core 0 parks the tick-based
        // timeout never fires.
        //
        // Fix: arm a TIMER0 alarm on core 1 that fires every 1 ms
        // independently of the FreeRTOS tick.  The ISR checks CORE0_PARKED
        // and sends to the park-signal queue from ISR context, waking the
        // PDB task even after the tick freezes.
        #[cfg(feature = "chip-rp2350-hal")]
        {
            // Arm the timer alarm
            crate::hal::timer_alarm::arm_park_alarm();

            // Block in FreeRTOS — lets core 0's scheduler run.
            // The timer alarm ISR will wake us via the park-signal queue
            // once CORE0_PARKED is set.
            for _ in 0..30u32 {
                if pending::CORE0_PARKED.load(Ordering::Acquire) {
                    crate::hal::timer_alarm::disarm_park_alarm();
                    return true;
                }
                pending::wait_park_signal(500);
            }
            crate::hal::timer_alarm::disarm_park_alarm();
            false
        }
        #[cfg(not(feature = "chip-rp2350-hal"))]
        {
            for _ in 0..1500u32 {
                if pending::CORE0_PARKED.load(Ordering::Acquire) {
                    return true;
                }
                CurrentTask::delay(Duration::ms(10));
            }
            false
        }
    }

    fn release(&mut self) {
        pending::CORE0_RELEASE.store(true, Ordering::Release);
    }

    fn cancel_park_request(&mut self) {
        pending::FLASH_PARK_REQUESTED.store(false, Ordering::Release);
    }
}
