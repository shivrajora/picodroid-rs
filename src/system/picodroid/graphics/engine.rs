//! Compatibility shim for the legacy `engine::*` API.
//!
//! The LVGL lifecycle now lives in [`super::lvgl::lifecycle`] behind the
//! [`super::gfx::Gfx`] trait. This module preserves the call shape used by
//! `app::run_jvm_with`, `display.rs`, `widgets/*.rs`, and `calibration.rs`
//! during the multi-step migration. Once widgets switch to
//! `with_gfx(|g| g.screen())` (plan step 7) and `display.rs` follows
//! (step 8), this file's screen accessor goes away (step 10).

use crate::hal;
use crate::lvgl_ffi::*;
use core::sync::atomic::{AtomicBool, Ordering};

use super::lvgl::{lifecycle, with_gfx};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize LVGL, the display, and the touch controller. Idempotent.
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);

    with_gfx(|g| g.init(hal::display::WIDTH, hal::display::HEIGHT));

    // Keypad indev + focus group + button GPIO setup — only on boards that
    // declare hardware buttons in board.toml. Moves to `lvgl::events` in
    // plan step 5.
    #[cfg(has_buttons)]
    unsafe {
        let keypad = lv_indev_create();
        lv_indev_set_type(keypad, LV_INDEV_TYPE_KEYPAD);
        lv_indev_set_read_cb(keypad, Some(keypad_read_cb));

        let group = lv_group_create();
        lv_group_set_default(group);
        lv_indev_set_group(keypad, group);
    }

    init_button_pins();
}

/// Advance LVGL's internal tick counter and process pending timers /
/// rendering. Call periodically (~16 ms for 60 fps).
pub fn tick(ms: u32) {
    with_gfx(|g| g.tick(ms));
}

/// Put the display panel into low-power sleep. LVGL state is untouched —
/// the caller is responsible for stopping `tick()` until `wake()`.
#[cfg_attr(any(feature = "sim", not(has_buttons)), allow(dead_code))]
pub fn sleep() {
    with_gfx(|g| g.sleep());
}

/// Bring the display back from sleep and force a full LVGL repaint.
#[cfg_attr(any(feature = "sim", not(has_buttons)), allow(dead_code))]
pub fn wake() {
    with_gfx(|g| g.wake());
}

/// Get the active screen object as a raw LVGL pointer.
///
/// Legacy accessor used by widgets that haven't been migrated to
/// `with_gfx(|g| g.screen())` yet (plan step 7). Goes away in step 10.
pub fn screen() -> *mut lv_obj_t {
    lifecycle::screen_ptr()
}

/// Re-export calibration entry point so existing `engine::calibrate()`
/// calls continue to work without changes.
pub use super::calibration::calibrate;

// ── Keypad input (interrupt-driven via GPIO ISR queue) ──────────────────────
//
// Moves to `lvgl::events` in plan step 5. Kept here meanwhile so the
// `system/native_handler` layer doesn't change shape.

// Board-specific button table generated from `[[button]]` in board.toml.
// Entries: (pin, LV_KEY_*, android_keycode). Empty on boards without buttons.
mod button_generated {
    #[allow(unused_imports)]
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/button_config.rs"));
}
use button_generated::BUTTONS;

/// Look up the Android keycode for a hardware button pin.
pub fn pin_to_keycode(pin: u8) -> Option<i32> {
    BUTTONS
        .iter()
        .find(|&&(p, _, _)| p == pin)
        .map(|&(_, _, k)| k)
}

#[cfg(has_buttons)]
fn init_button_pins() {
    for &(pin, _, _) in BUTTONS {
        hal::gpio::set_input(pin, hal::gpio::Pull::Up);
        hal::gpio::enable_edge_irq(pin, hal::gpio::EdgeTrigger::Both);
    }
    hal::gpio::init_gpio_irq();
}

#[cfg(not(has_buttons))]
fn init_button_pins() {}

// ── Java-visible key event queue (parallel to LVGL's internal queue) ────────

#[derive(Copy, Clone)]
pub struct KeyEventRaw {
    pub pin: u8,
    pub rising: bool,
}

const KEY_EVENT_QUEUE_SIZE: usize = 16;
static mut KEY_EVENT_QUEUE: [KeyEventRaw; KEY_EVENT_QUEUE_SIZE] = [KeyEventRaw {
    pin: 0,
    rising: false,
}; KEY_EVENT_QUEUE_SIZE];
static mut KEY_EVENT_QUEUE_HEAD: usize = 0;
static mut KEY_EVENT_QUEUE_TAIL: usize = 0;

#[cfg(has_buttons)]
fn push_key_event_raw(pin: u8, rising: bool) {
    unsafe {
        let head = KEY_EVENT_QUEUE_HEAD;
        let next = (head + 1) % KEY_EVENT_QUEUE_SIZE;
        if next != KEY_EVENT_QUEUE_TAIL {
            KEY_EVENT_QUEUE[head] = KeyEventRaw { pin, rising };
            KEY_EVENT_QUEUE_HEAD = next;
        }
    }
}

/// Pop one key event from the Java-visible queue, if any.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_key_event() -> Option<KeyEventRaw> {
    unsafe {
        if KEY_EVENT_QUEUE_TAIL == KEY_EVENT_QUEUE_HEAD {
            return None;
        }
        let event = KEY_EVENT_QUEUE[KEY_EVENT_QUEUE_TAIL];
        KEY_EVENT_QUEUE_TAIL = (KEY_EVENT_QUEUE_TAIL + 1) % KEY_EVENT_QUEUE_SIZE;
        Some(event)
    }
}

/// Clear the key event queue between app runs.
pub fn reset_key_event_queue() {
    unsafe {
        KEY_EVENT_QUEUE_HEAD = 0;
        KEY_EVENT_QUEUE_TAIL = 0;
    }
}

/// Return the Java `View` object reference for LVGL's currently focused
/// widget, if one is registered as a key listener via
/// `view::register_key_listener`.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn focused_view_obj() -> Option<u16> {
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return None;
        }
        let focused = lv_group_get_focused(group);
        if focused.is_null() {
            return None;
        }
        super::view::lookup_view_obj(focused as usize)
    }
}

#[cfg(has_buttons)]
unsafe extern "C" fn keypad_read_cb(_indev: *mut lv_indev_t, data: *mut lv_indev_data_t) {
    let d = unsafe { &mut *data };
    if let Some(event) = hal::gpio::drain_gpio_event() {
        push_key_event_raw(event.pin, event.rising);

        let key = BUTTONS
            .iter()
            .find(|&&(p, _, _)| p == event.pin)
            .map(|&(_, k, _)| k);
        if let Some(k) = key {
            d.key = k;
            d.state = if event.rising {
                LV_INDEV_STATE_RELEASED
            } else {
                LV_INDEV_STATE_PRESSED
            };
        }
        d.continue_reading = hal::gpio::has_pending_event();
    } else {
        d.state = LV_INDEV_STATE_RELEASED;
        d.continue_reading = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(has_buttons)]
    #[test]
    fn pin_to_keycode_roundtrips_declared_pins() {
        for &(pin, _, keycode) in BUTTONS {
            assert_eq!(pin_to_keycode(pin), Some(keycode));
        }
    }

    #[test]
    fn pin_to_keycode_returns_none_for_unmapped() {
        assert_eq!(pin_to_keycode(99), None);
    }

    #[cfg(has_buttons)]
    #[test]
    fn key_event_queue_roundtrips_in_fifo_order() {
        reset_key_event_queue();
        push_key_event_raw(12, false);
        push_key_event_raw(13, true);
        push_key_event_raw(14, false);

        let a = drain_key_event().unwrap();
        assert_eq!(a.pin, 12);
        assert!(!a.rising);
        let b = drain_key_event().unwrap();
        assert_eq!(b.pin, 13);
        assert!(b.rising);
        let c = drain_key_event().unwrap();
        assert_eq!(c.pin, 14);
        assert!(!c.rising);
        assert!(drain_key_event().is_none());
    }

    #[cfg(has_buttons)]
    #[test]
    fn key_event_queue_wraps_around() {
        reset_key_event_queue();
        for cycle in 0..4 {
            for i in 0..KEY_EVENT_QUEUE_SIZE - 1 {
                push_key_event_raw(i as u8, cycle % 2 == 0);
            }
            for i in 0..KEY_EVENT_QUEUE_SIZE - 1 {
                let e = drain_key_event().unwrap();
                assert_eq!(e.pin, i as u8);
            }
            assert!(drain_key_event().is_none());
        }
    }
}
