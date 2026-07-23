// SPDX-License-Identifier: GPL-3.0-only
//! CMD_INPUT handler — inject synthetic button / touch input from the host.
//!
//! The picodroid analog of `adb shell input tap|swipe|keyevent`. The host sends
//! a compact verb; this handler turns it into HAL-level input — a GPIO edge via
//! [`crate::hal::gpio::inject`], or a touch sample via the `hal::touch` override
//! — so the *whole* on-device pipeline runs exactly as it does for real input:
//! EditMode filter, phantom-release filter, LVGL keypad indev + focus nav,
//! `pin_to_keycode`, BACK routing, and touch hit-test / gesture / `MotionEvent`
//! dispatch. Faithful to Android's `InputManager.injectInputEvent(…,
//! WAIT_FOR_FINISH)`: the device builds the event, injection is privileged
//! (reachable only over PDB), and the handler blocks until the gesture is
//! delivered before replying.

use freertos_rust::{CurrentTask, Duration};

use super::cdc_transport::CdcTransport;
use super::protocol::{
    crc32_frame, CMD_INPUT, INPUT_KEY, INPUT_SWIPE, INPUT_TAP, KEY_META_DOWN, KEY_META_DOWN_UP,
    KEY_META_UP, STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK,
};
use crate::system::picodroid::graphics::lvgl::events::keycode_to_pin;

/// Max CMD_INPUT payload: SWIPE = 1 subtype + 4×i32 + u32 = 21 bytes. Rounded up.
const MAX_PAYLOAD: usize = 24;

/// Gap between a key PRESS and its RELEASE so the two edges land in distinct
/// LVGL ticks (the keypad indev drains one edge per read). Mirrors the 40 ms
/// the sim control channel sleeps between press and release.
const KEY_EDGE_GAP_MS: u32 = 40;

/// Hold time before a tap/swipe DOWN is released, and the post-release settle.
/// `touch_read_cb` discards the first unsettled sample, so a press must survive
/// ≥2 poll cycles (~16 ms/frame) to register as ACTION_DOWN.
#[cfg(has_touch)]
const TAP_HOLD_MS: u32 = 120;
#[cfg(has_touch)]
const TOUCH_SETTLE_MS: u32 = 80;
/// Settle after the initial swipe press so DOWN registers before the first MOVE.
#[cfg(has_touch)]
const SWIPE_DOWN_SETTLE_MS: u32 = 40;
/// Intermediate MOVE samples emitted across a swipe's duration.
#[cfg(has_touch)]
const SWIPE_STEPS: u32 = 12;

pub fn handle_input(len: u32) {
    // Drain the framed payload (bounded) + trailing CRC, keeping the byte
    // stream in sync even if the payload is malformed or oversized.
    let mut payload = [0u8; MAX_PAYLOAD];
    let n = (len as usize).min(MAX_PAYLOAD);
    for b in payload.iter_mut().take(n) {
        *b = crate::hal::pdb_usb::queue_read_byte();
    }
    for _ in MAX_PAYLOAD..len as usize {
        let _ = crate::hal::pdb_usb::queue_read_byte(); // discard overflow
    }
    let wire_crc = crate::hal::pdb_usb::queue_read_u32_le();

    if wire_crc != crc32_frame(CMD_INPUT, len, &payload[..n]) {
        CdcTransport::send_pdbp_response(STATUS_CRC_FAIL, b"");
        return;
    }
    if n == 0 {
        CdcTransport::send_pdbp_response(STATUS_ERR, b"empty input");
        return;
    }

    let args = &payload[1..n];
    let (status, msg): (u8, &[u8]) = match payload[0] {
        INPUT_KEY => inject_key(args),
        INPUT_TAP => inject_tap(args),
        INPUT_SWIPE => inject_swipe(args),
        _ => (STATUS_ERR, b"bad input subtype"),
    };
    CdcTransport::send_pdbp_response(status, msg);
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
    if meta != KEY_META_UP {
        crate::hal::gpio::inject(pin, false); // PRESS
    }
    if meta == KEY_META_DOWN_UP {
        CurrentTask::delay(Duration::ms(KEY_EDGE_GAP_MS));
    }
    if meta != KEY_META_DOWN {
        crate::hal::gpio::inject(pin, true); // RELEASE
    }
    (STATUS_OK, b"")
}

// ── Touch injection ──────────────────────────────────────────────────────────

/// Clamp a host-sent coordinate into `[0, max-1]`.
#[cfg(has_touch)]
fn clamp_coord(v: i32, max: u16) -> u16 {
    v.clamp(0, (max.saturating_sub(1)) as i32) as u16
}

/// Linear interpolation between two on-screen coordinates (both valid `u16`,
/// so the result stays in `[min(a,b), max(a,b)]` and the cast is lossless).
#[cfg(has_touch)]
fn lerp(a: u16, b: u16, i: u32, n: u32) -> u16 {
    let (a, b) = (a as i32, b as i32);
    (a + (b - a) * i as i32 / n as i32) as u16
}

/// TAP payload: `[x: i32 LE][y: i32 LE]`. Press → hold → release at one point.
#[cfg(has_touch)]
fn inject_tap(args: &[u8]) -> (u8, &'static [u8]) {
    let (Some(x), Some(y)) = (rd_i32(args, 0), rd_i32(args, 4)) else {
        return (STATUS_ERR, b"tap: short payload");
    };
    let x = clamp_coord(x, crate::hal::display::WIDTH);
    let y = clamp_coord(y, crate::hal::display::HEIGHT);
    crate::hal::touch::inject_override(x, y);
    CurrentTask::delay(Duration::ms(TAP_HOLD_MS));
    crate::hal::touch::release_override();
    CurrentTask::delay(Duration::ms(TOUCH_SETTLE_MS));
    crate::hal::touch::clear_override();
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
    let (x1, y1) = (clamp_coord(x1, w), clamp_coord(y1, h));
    let (x2, y2) = (clamp_coord(x2, w), clamp_coord(y2, h));
    let step_ms = (dur / SWIPE_STEPS).max(1);

    crate::hal::touch::inject_override(x1, y1); // DOWN at start
    CurrentTask::delay(Duration::ms(SWIPE_DOWN_SETTLE_MS));
    for i in 1..=SWIPE_STEPS {
        crate::hal::touch::inject_override(
            lerp(x1, x2, i, SWIPE_STEPS),
            lerp(y1, y2, i, SWIPE_STEPS),
        );
        CurrentTask::delay(Duration::ms(step_ms));
    }
    crate::hal::touch::release_override();
    CurrentTask::delay(Duration::ms(TOUCH_SETTLE_MS));
    crate::hal::touch::clear_override();
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
