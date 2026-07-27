// SPDX-License-Identifier: GPL-3.0-only
//! PDBP response framing, and the adapter that lets the install path speak it.

use pdb_protocol::{
    FRAME_MAGIC, STATUS_CRC_FAIL, STATUS_ERR, STATUS_INCOMPAT, STATUS_OK, STATUS_READY,
    STATUS_TOO_LARGE,
};

use super::PdbTransport;
use crate::install::{InstallError, InstallTransport, ReadError};

/// Largest payload that fits the single-write path. Bigger ones are split,
/// which is correct but costs a second transfer, so the buffer is sized for
/// the largest response actually sent (a full sysmon table).
const FRAME_BUF: usize = 9 + 512;

/// Send `[PDBP][status][len: u32 LE][payload]`.
///
/// No CRC on responses: they travel over the same error-checked pipe as the
/// request, and the host has no way to ask for a retransmit — a mismatch
/// would leave it no better off than a short read.
pub fn send_response(transport: &mut impl PdbTransport, status: u8, payload: &[u8]) {
    let len = payload.len() as u32;
    let mut buf = [0u8; FRAME_BUF];
    buf[..4].copy_from_slice(FRAME_MAGIC);
    buf[4] = status;
    buf[5..9].copy_from_slice(&len.to_le_bytes());

    let inline = payload.len().min(FRAME_BUF - 9);
    buf[9..9 + inline].copy_from_slice(&payload[..inline]);
    transport.write_bytes(&buf[..9 + inline]);

    // Only for a payload past the buffer; the header has already gone out, so
    // the remainder is just more bytes on the same frame.
    if payload.len() > inline {
        transport.write_bytes(&payload[inline..]);
    }
}

/// A [`PdbTransport`] presented to the install path as an [`InstallTransport`].
///
/// The install orchestrator is deliberately transport-agnostic — it could be
/// driven over BLE, OTA or an SD card — so it speaks in reads and outcomes
/// rather than PDBP status bytes. This is where the two meet, and it is the
/// only place that knows an install error has a wire representation.
pub struct Framed<'a, T: PdbTransport>(pub &'a mut T);

impl<T: PdbTransport> InstallTransport for Framed<'_, T> {
    fn read_byte(&mut self) -> Result<u8, ReadError> {
        self.0.read_byte_timeout().ok_or(ReadError::Timeout)
    }

    fn report_ready(&mut self) {
        send_response(self.0, STATUS_READY, b"");
    }

    fn report_success(&mut self) {
        send_response(self.0, STATUS_OK, b"");
        // The reset follows immediately; anything still buffered dies with it.
        self.0.drain_tx();
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
        send_response(self.0, status, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdb::tests::MockPipe;
    use alloc::vec::Vec;

    fn framed_bytes(status: u8, payload: &[u8]) -> Vec<u8> {
        let mut p = MockPipe::new(Vec::new());
        send_response(&mut p, status, payload);
        p.tx
    }

    #[test]
    fn response_header_is_magic_status_len() {
        let tx = framed_bytes(STATUS_OK, b"hi");
        assert_eq!(&tx[..4], FRAME_MAGIC);
        assert_eq!(tx[4], STATUS_OK);
        assert_eq!(&tx[5..9], &2u32.to_le_bytes());
        assert_eq!(&tx[9..], b"hi");
    }

    #[test]
    fn an_empty_payload_still_sends_a_full_header() {
        let tx = framed_bytes(STATUS_READY, b"");
        assert_eq!(tx.len(), 9);
        assert_eq!(&tx[5..9], &0u32.to_le_bytes());
    }

    /// A payload past the inline buffer is split across two writes, but the
    /// bytes on the wire must be identical to the unsplit case — the declared
    /// length covers the whole thing, so a truncating bug here would desync
    /// the host mid-stream rather than fail visibly.
    #[test]
    fn an_oversized_payload_is_split_without_loss() {
        let payload: Vec<u8> = (0..(FRAME_BUF * 2)).map(|i| (i % 251) as u8).collect();
        let tx = framed_bytes(STATUS_OK, &payload);
        assert_eq!(&tx[5..9], &(payload.len() as u32).to_le_bytes());
        assert_eq!(&tx[9..], &payload[..]);
    }

    /// Every install outcome must map to a distinct-enough status that the
    /// host can tell "your PAPK is wrong" from "the wire broke" — the two
    /// call for different user action.
    #[test]
    fn install_errors_map_to_their_wire_statuses() {
        for (err, want) in [
            (InstallError::TooLarge, STATUS_TOO_LARGE),
            (InstallError::CrcMismatch, STATUS_CRC_FAIL),
            (InstallError::Incompat, STATUS_INCOMPAT),
            (InstallError::ParkTimeout, STATUS_ERR),
        ] {
            let mut p = MockPipe::new(Vec::new());
            Framed(&mut p).report_error(err);
            assert_eq!(p.tx[4], want);
        }
    }

    #[test]
    fn a_timed_out_read_surfaces_as_a_timeout() {
        let mut p = MockPipe::new(Vec::new()); // nothing to read
        assert!(matches!(
            Framed(&mut p).read_byte(),
            Err(ReadError::Timeout)
        ));
    }
}
