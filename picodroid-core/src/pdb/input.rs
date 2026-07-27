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

use pdb_protocol::{
    crc32_frame, CMD_INPUT, INPUT_KEY, INPUT_SWIPE, INPUT_TAP, KEY_META_DOWN, KEY_META_DOWN_UP,
    KEY_META_UP, STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK,
};

use crate::board_cfg::buttons::keycode_to_pin;
use crate::input_inject;

use super::{send_response, PdbTransport};

/// Max CMD_INPUT payload: SWIPE = 1 subtype + 4×i32 + u32 = 21 bytes. Rounded up.
const MAX_PAYLOAD: usize = 24;

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
    let mut payload = [0u8; MAX_PAYLOAD];
    let n = (len as usize).min(MAX_PAYLOAD);
    for b in payload.iter_mut().take(n) {
        *b = transport.read_byte();
    }
    for _ in MAX_PAYLOAD..len as usize {
        let _ = transport.read_byte(); // discard overflow
    }
    let wire_crc = transport.read_u32_le();

    if wire_crc != crc32_frame(CMD_INPUT, len, &payload[..n]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }
    if n == 0 {
        send_response(transport, STATUS_ERR, b"empty input");
        return;
    }

    let args = &payload[1..n];
    let (status, msg): (u8, &[u8]) = match payload[0] {
        INPUT_KEY => inject_key(args),
        INPUT_TAP => inject_tap(args),
        INPUT_SWIPE => inject_swipe(args),
        _ => (STATUS_ERR, b"bad input subtype"),
    };
    send_response(transport, status, msg);
}

// ── Little-endian slice readers ──────────────────────────────────────────────

fn rd_i32(b: &[u8], off: usize) -> Option<i32> {
    let s = b.get(off..off + 4)?;
    Some(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

#[cfg(has_touch)]
fn rd_u32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

// ── Key injection ────────────────────────────────────────────────────────────

/// KEY payload: `[keycode: i32 LE][meta: u8]`. Resolves the Android keycode to a
/// board button pin and injects edges. Active-low: PRESS = falling
/// (`rising=false`), RELEASE = rising.
fn inject_key(args: &[u8]) -> (u8, &'static [u8]) {
    let Some(keycode) = rd_i32(args, 0) else {
        return (STATUS_ERR, b"key: short payload");
    };
    let meta = args.get(4).copied().unwrap_or(KEY_META_DOWN_UP);
    let Some(pin) = keycode_to_pin(keycode) else {
        return (STATUS_ERR, b"no such key");
    };
    match meta {
        KEY_META_DOWN => input_inject::press::<HalSink>(pin),
        KEY_META_UP => input_inject::release::<HalSink>(pin),
        _ => input_inject::press_release::<HalSink>(pin),
    }
    (STATUS_OK, b"")
}

// ── Touch injection ──────────────────────────────────────────────────────────

/// TAP payload: `[x: i32 LE][y: i32 LE]`. Press → hold → release at one point.
#[cfg(has_touch)]
fn inject_tap(args: &[u8]) -> (u8, &'static [u8]) {
    let (Some(x), Some(y)) = (rd_i32(args, 0), rd_i32(args, 4)) else {
        return (STATUS_ERR, b"tap: short payload");
    };
    let x = input_inject::clamp_coord(x, crate::hal::display::WIDTH);
    let y = input_inject::clamp_coord(y, crate::hal::display::HEIGHT);
    input_inject::tap::<HalSink>(x, y);
    (STATUS_OK, b"")
}

/// SWIPE payload: `[x1][y1][x2][y2][duration_ms: u32]` (all LE). Press at start,
/// step interpolated MOVEs to the end over `duration_ms`, then release.
#[cfg(has_touch)]
fn inject_swipe(args: &[u8]) -> (u8, &'static [u8]) {
    let (Some(x1), Some(y1), Some(x2), Some(y2), Some(dur)) = (
        rd_i32(args, 0),
        rd_i32(args, 4),
        rd_i32(args, 8),
        rd_i32(args, 12),
        rd_u32(args, 16),
    ) else {
        return (STATUS_ERR, b"swipe: short payload");
    };
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
    input_inject::swipe::<HalSink>(x1, y1, x2, y2, dur);
    (STATUS_OK, b"")
}

#[cfg(not(has_touch))]
fn inject_tap(_args: &[u8]) -> (u8, &'static [u8]) {
    (STATUS_ERR, b"no touch panel")
}

#[cfg(not(has_touch))]
fn inject_swipe(_args: &[u8]) -> (u8, &'static [u8]) {
    (STATUS_ERR, b"no touch panel")
}
