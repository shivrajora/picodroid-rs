// SPDX-License-Identifier: GPL-3.0-only
//! `CMD_INPUT` — inject synthetic button / touch input from the host, the
//! picodroid analog of `adb shell input tap|swipe|keyevent`.
//!
//! The payload bytes are `pdb_protocol::input`'s. This module reads the
//! frame, checks its CRC, decodes the event and hands it to
//! [`PdbDevice::inject`], which owns everything a wire format cannot know —
//! keycode→pin resolution, coordinate clamping, whether this board has a
//! touch panel at all — and blocks until the gesture is delivered, as
//! Android's `InputManager.injectInputEvent(…, WAIT_FOR_FINISH)` does.

use pdb_protocol::input::{InputError, InputEvent, MAX_INPUT_PAYLOAD};
use pdb_protocol::{
    crc32_frame, CMD_INPUT, INPUT_KEY, INPUT_SWIPE, INPUT_TAP, STATUS_CRC_FAIL, STATUS_ERR,
};

use crate::{send_response, PdbDevice, PdbTransport};

pub fn handle(transport: &mut impl PdbTransport, device: &mut impl PdbDevice, len: u32) {
    // The payload + trailing CRC. `serve_one` refused any `len` above
    // MAX_INPUT_PAYLOAD before calling here; the `min` keeps this function
    // safe on its own.
    let mut payload = [0u8; MAX_INPUT_PAYLOAD];
    let n = (len as usize).min(MAX_INPUT_PAYLOAD);
    for b in payload.iter_mut().take(n) {
        *b = transport.read_byte();
    }
    let wire_crc = transport.read_u32_le();

    if wire_crc != crc32_frame(CMD_INPUT, len, &payload[..n]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }

    let (status, msg): (u8, &[u8]) = match InputEvent::decode(&payload[..n]) {
        Ok(event) => device.inject(event),
        Err(InputError::Empty) => (STATUS_ERR, b"empty input"),
        Err(InputError::Short(INPUT_KEY)) => (STATUS_ERR, b"key: short payload"),
        Err(InputError::Short(INPUT_TAP)) => (STATUS_ERR, b"tap: short payload"),
        Err(InputError::Short(INPUT_SWIPE)) => (STATUS_ERR, b"swipe: short payload"),
        Err(_) => (STATUS_ERR, b"bad input subtype"),
    };
    send_response(transport, status, msg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_device::Stub;
    use crate::vec_pipe::VecPipe;
    use alloc::vec::Vec;
    use pdb_protocol::STATUS_OK;

    /// A complete host-side CMD_INPUT frame body (payload + CRC), as
    /// `tools/pdb` would put it on the wire after the magic/cmd/len header
    /// the dispatch loop consumes before calling `handle`.
    fn frame_body(payload: &[u8]) -> Vec<u8> {
        let mut rx = Vec::from(payload);
        rx.extend_from_slice(&crc32_frame(CMD_INPUT, payload.len() as u32, payload).to_le_bytes());
        rx
    }

    fn response_of(payload: &[u8]) -> (u8, Vec<u8>) {
        let mut p = VecPipe::new(frame_body(payload));
        let mut device = Stub::<true>::new(8);
        handle(&mut p, &mut device, payload.len() as u32);
        let n = u32::from_le_bytes(p.tx[5..9].try_into().unwrap()) as usize;
        (p.tx[4], p.tx[9..9 + n].to_vec())
    }

    #[test]
    fn a_bad_crc_is_rejected_before_decoding() {
        let mut rx = Vec::from(&[INPUT_KEY, 4, 0, 0, 0, 0][..]);
        rx.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        let mut p = VecPipe::new(rx);
        let mut device = Stub::<true>::new(8);
        handle(&mut p, &mut device, 6);
        assert_eq!(p.tx[4], STATUS_CRC_FAIL);
        assert!(device.injected.is_empty());
    }

    /// Host-encoded malformed payloads come back with the exact per-verb
    /// messages — the decode errors carry their subtype so the wording
    /// stays what it was when the parser lived here.
    #[test]
    fn decode_failures_answer_with_the_verbs_own_message() {
        assert_eq!(
            response_of(&[]),
            (STATUS_ERR, Vec::from(&b"empty input"[..]))
        );
        assert_eq!(
            response_of(&[INPUT_KEY, 1]),
            (STATUS_ERR, Vec::from(&b"key: short payload"[..]))
        );
        assert_eq!(
            response_of(&[INPUT_TAP, 1, 2, 3, 4]),
            (STATUS_ERR, Vec::from(&b"tap: short payload"[..]))
        );
        assert_eq!(
            response_of(&[INPUT_SWIPE]),
            (STATUS_ERR, Vec::from(&b"swipe: short payload"[..]))
        );
        assert_eq!(
            response_of(&[0x7F]),
            (STATUS_ERR, Vec::from(&b"bad input subtype"[..]))
        );
    }

    /// A well-formed event reaches the device, which answers.
    #[test]
    fn a_decoded_event_is_the_devices_to_answer() {
        let mut buf = [0u8; MAX_INPUT_PAYLOAD];
        let n = InputEvent::Tap { x: 10, y: 20 }.encode(&mut buf);
        let mut p = VecPipe::new(frame_body(&buf[..n]));
        let mut device = Stub::<true>::new(8);
        handle(&mut p, &mut device, n as u32);
        assert_eq!(p.tx[4], STATUS_OK);
        assert_eq!(device.injected, [InputEvent::Tap { x: 10, y: 20 }]);
    }
}
