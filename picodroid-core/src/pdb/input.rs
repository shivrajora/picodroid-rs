// SPDX-License-Identifier: GPL-3.0-only
//! CMD_INPUT handler — inject synthetic button / touch input from the host.
//!
//! The picodroid analog of `adb shell input tap|swipe|keyevent`. The host sends
//! a compact verb; this handler turns it into HAL-level input — a GPIO edge
//! via [`crate::hal::gpio::inject`], or a touch sample via the `hal::touch`
//! override — so the *whole* on-device pipeline runs exactly as it does for
//! real input:
//! EditMode filter, phantom-release filter, LVGL keypad indev + focus nav,
//! `pin_to_keycode`, BACK routing, and touch hit-test / gesture / `MotionEvent`
//! dispatch. Faithful to Android's `InputManager.injectInputEvent(…,
//! WAIT_FOR_FINISH)`: the device builds the event, injection is privileged
//! (reachable only over PDB), and the handler blocks until the gesture is
//! delivered before replying.
//!
//! The payload bytes are `pdb_protocol::input`'s; what this module owns is
//! everything a wire format cannot know — keycode→pin resolution, coordinate
//! clamping, and whether this board has a touch panel at all.

use pdb_protocol::input::{InputError, InputEvent, MAX_INPUT_PAYLOAD};
use pdb_protocol::{
    crc32_frame, CMD_INPUT, INPUT_KEY, INPUT_SWIPE, INPUT_TAP, KEY_META_DOWN, KEY_META_UP,
    STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK,
};

use crate::board_cfg::buttons::keycode_to_pin;
use crate::input_inject;

use super::{send_response, PdbTransport};

/// Injects through the HAL facade, which is the right route on hardware.
/// (The simulator's front-end uses its own sink — see
/// [`crate::input_inject`] for why the two differ.)
struct HalSink;

impl input_inject::InputSink for HalSink {
    fn gpio_inject(pin: u8, rising: bool) {
        crate::hal::gpio::inject(pin, rising)
    }
    fn touch_set(x: u16, y: u16) {
        crate::hal::touch::inject_override(x, y)
    }
    fn touch_release() {
        crate::hal::touch::release_override()
    }
    fn touch_clear() {
        crate::hal::touch::clear_override()
    }
    fn delay_ms(ms: u32) {
        crate::rtos::delay_ms(ms)
    }
}

pub fn handle(transport: &mut impl PdbTransport, len: u32) {
    // Drain the framed payload (bounded) + trailing CRC, keeping the byte
    // stream in sync even if the payload is malformed or oversized.
    let mut payload = [0u8; MAX_INPUT_PAYLOAD];
    let n = (len as usize).min(MAX_INPUT_PAYLOAD);
    for b in payload.iter_mut().take(n) {
        *b = transport.read_byte();
    }
    for _ in MAX_INPUT_PAYLOAD..len as usize {
        let _ = transport.read_byte(); // discard overflow
    }
    let wire_crc = transport.read_u32_le();

    if wire_crc != crc32_frame(CMD_INPUT, len, &payload[..n]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }

    let (status, msg): (u8, &[u8]) = match InputEvent::decode(&payload[..n]) {
        Ok(InputEvent::Key { keycode, meta }) => inject_key(keycode, meta),
        Ok(InputEvent::Tap { x, y }) => inject_tap(x, y),
        Ok(InputEvent::Swipe {
            x1,
            y1,
            x2,
            y2,
            duration_ms,
        }) => inject_swipe(x1, y1, x2, y2, duration_ms),
        Err(InputError::Empty) => (STATUS_ERR, b"empty input"),
        Err(InputError::Short(INPUT_KEY)) => (STATUS_ERR, b"key: short payload"),
        Err(InputError::Short(INPUT_TAP)) => (STATUS_ERR, b"tap: short payload"),
        Err(InputError::Short(INPUT_SWIPE)) => (STATUS_ERR, b"swipe: short payload"),
        Err(_) => (STATUS_ERR, b"bad input subtype"),
    };
    send_response(transport, status, msg);
}

// ── Key injection ────────────────────────────────────────────────────────────

/// Resolves the Android keycode to a board button pin and injects edges.
/// Active-low: PRESS = falling (`rising=false`), RELEASE = rising.
fn inject_key(keycode: i32, meta: u8) -> (u8, &'static [u8]) {
    let Some(pin) = keycode_to_pin(keycode) else {
        return (STATUS_ERR, b"no such key");
    };
    crate::pd_info!("pdb: key {} -> pin {}", keycode, pin);
    match meta {
        KEY_META_DOWN => input_inject::press::<HalSink>(pin),
        KEY_META_UP => input_inject::release::<HalSink>(pin),
        _ => input_inject::press_release::<HalSink>(pin),
    }
    (STATUS_OK, b"")
}

// ── Touch injection ──────────────────────────────────────────────────────────

/// Press → hold → release at one point.
#[cfg(has_touch)]
fn inject_tap(x: i32, y: i32) -> (u8, &'static [u8]) {
    let x = input_inject::clamp_coord(x, crate::hal::display::WIDTH);
    let y = input_inject::clamp_coord(y, crate::hal::display::HEIGHT);
    input_inject::tap::<HalSink>(x, y);
    (STATUS_OK, b"")
}

/// Press at start, step interpolated MOVEs to the end over `duration_ms`,
/// then release.
#[cfg(has_touch)]
fn inject_swipe(x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u32) -> (u8, &'static [u8]) {
    let w = crate::hal::display::WIDTH;
    let h = crate::hal::display::HEIGHT;
    let (x1, y1) = (
        input_inject::clamp_coord(x1, w),
        input_inject::clamp_coord(y1, h),
    );
    let (x2, y2) = (
        input_inject::clamp_coord(x2, w),
        input_inject::clamp_coord(y2, h),
    );
    input_inject::swipe::<HalSink>(x1, y1, x2, y2, duration_ms);
    (STATUS_OK, b"")
}

#[cfg(not(has_touch))]
fn inject_tap(_x: i32, _y: i32) -> (u8, &'static [u8]) {
    (STATUS_ERR, b"no touch panel")
}

#[cfg(not(has_touch))]
fn inject_swipe(_x1: i32, _y1: i32, _x2: i32, _y2: i32, _duration_ms: u32) -> (u8, &'static [u8]) {
    (STATUS_ERR, b"no touch panel")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdb::tests::MockPipe;
    use alloc::vec::Vec;

    /// A complete host-side CMD_INPUT frame body (payload + CRC), as
    /// `tools/pdb` would put it on the wire after the magic/cmd/len header
    /// the dispatch loop consumes before calling `handle`.
    fn frame_body(payload: &[u8]) -> Vec<u8> {
        let mut rx = Vec::from(payload);
        rx.extend_from_slice(&crc32_frame(CMD_INPUT, payload.len() as u32, payload).to_le_bytes());
        rx
    }

    fn response_of(payload: &[u8]) -> (u8, Vec<u8>) {
        let mut p = MockPipe::new(frame_body(payload));
        handle(&mut p, payload.len() as u32);
        let n = u32::from_le_bytes(p.tx[5..9].try_into().unwrap()) as usize;
        (p.tx[4], p.tx[9..9 + n].to_vec())
    }

    #[test]
    fn a_bad_crc_is_rejected_before_decoding() {
        let mut rx = Vec::from(&[INPUT_KEY, 4, 0, 0, 0, 0][..]);
        rx.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        let mut p = MockPipe::new(rx);
        handle(&mut p, 6);
        assert_eq!(p.tx[4], STATUS_CRC_FAIL);
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

    /// A shared-encoder keyevent for a keycode no board maps reaches the
    /// handler's own policy answer — the wire round trip works end to end
    /// without injecting anything.
    #[test]
    fn an_unmapped_keycode_is_the_handlers_answer_not_the_decoders() {
        let mut buf = [0u8; MAX_INPUT_PAYLOAD];
        let n = InputEvent::Key {
            keycode: 999,
            meta: pdb_protocol::KEY_META_DOWN_UP,
        }
        .encode(&mut buf);
        assert_eq!(
            response_of(&buf[..n]),
            (STATUS_ERR, Vec::from(&b"no such key"[..]))
        );
    }
}
