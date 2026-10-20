// SPDX-License-Identifier: GPL-3.0-only
//! CMD_INPUT's device half — inject synthetic button / touch input from
//! the host, the picodroid analog of `adb shell input tap|swipe|keyevent`.
//!
//! The frame is read and the payload decoded in `pd_pdb::input`; this
//! module turns the decoded event into HAL-level input — a GPIO edge via
//! [`crate::hal::gpio::inject`], or a touch sample via the `hal::touch`
//! override — so the *whole* on-device pipeline runs exactly as it does for
//! real input: EditMode filter, phantom-release filter, LVGL keypad indev +
//! focus nav, `pin_to_keycode`, BACK routing, and touch hit-test / gesture /
//! `MotionEvent` dispatch. Faithful to Android's
//! `InputManager.injectInputEvent(…, WAIT_FOR_FINISH)`: the device builds
//! the event, injection is privileged (reachable only over PDB), and the
//! handler blocks until the gesture is delivered before replying.
//!
//! What this module owns is everything a wire format cannot know —
//! keycode→pin resolution, coordinate clamping, and whether this board has
//! a touch panel at all.

use pdb_protocol::input::InputEvent;
#[cfg(not(has_touch))]
use pdb_protocol::STATUS_ERR;
use pdb_protocol::{KEY_META_DOWN, KEY_META_LONG_PRESS, KEY_META_UP, STATUS_OK};

use crate::board_cfg::buttons::keycode_to_pin;
use crate::input_inject;

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

/// Inject one decoded event; the status and message the frame is answered
/// with.
pub fn inject(event: InputEvent) -> (u8, &'static [u8]) {
    match event {
        InputEvent::Key { keycode, meta } => inject_key(keycode, meta),
        InputEvent::Tap { x, y } => inject_tap(x, y),
        InputEvent::Swipe {
            x1,
            y1,
            x2,
            y2,
            duration_ms,
        } => inject_swipe(x1, y1, x2, y2, duration_ms),
    }
}

// ── Key injection ────────────────────────────────────────────────────────────

/// Resolves the Android keycode to a board button pin and injects edges.
/// Active-low: PRESS = falling (`rising=false`), RELEASE = rising. A key
/// this board has no pin for goes through the soft-key queue instead
/// (`input_inject::key_soft`): it reaches the framework and the app as a
/// `KeyEvent` without touching LVGL's focus ring, which is what lets a
/// script send BACK or HOME to a board that has neither button.
fn inject_key(keycode: i32, meta: u8) -> (u8, &'static [u8]) {
    let hold = match meta {
        KEY_META_DOWN => input_inject::KeyHold::Down,
        KEY_META_UP => input_inject::KeyHold::Up,
        KEY_META_LONG_PRESS => input_inject::KeyHold::LongPress,
        _ => input_inject::KeyHold::PressRelease,
    };
    match keycode_to_pin(keycode) {
        Some(pin) => {
            crate::pd_info!("pdb: key {} -> pin {}", keycode, pin);
            input_inject::key::<HalSink>(pin, keycode, hold);
        }
        None => {
            crate::pd_info!("pdb: key {} -> soft (no pin on this board)", keycode);
            input_inject::key_soft(keycode, hold);
        }
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

    /// A keyevent for a keycode no board pin carries reaches this module's
    /// own policy answer — the soft-key queue (app-portability K8),
    /// accepted like Android's `input keyevent` accepts any code.
    #[test]
    fn an_unmapped_keycode_goes_to_the_soft_key_queue() {
        assert_eq!(
            inject(InputEvent::Key {
                keycode: 999,
                meta: pdb_protocol::KEY_META_DOWN_UP,
            }),
            (STATUS_OK, &b""[..])
        );
        assert!(input_inject::soft_key_pending());
        input_inject::reset_soft_keys();
    }
}
