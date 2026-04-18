//! LVGL display engine — manages the LVGL lifecycle, display, and touch input.

use crate::hal;
use crate::lvgl_ffi::*;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

/// Band buffer for LVGL partial rendering. Size and band height are
/// board-specific (set in the board config / hal::display constants).
const BAND_HEIGHT: usize = hal::display::BAND_HEIGHT;
const BAND_BUF_SIZE: usize = hal::display::WIDTH as usize * BAND_HEIGHT * 2;

/// Use a wrapper to get a raw pointer without creating a mutable reference.
/// Must be 4-byte aligned to satisfy LVGL's LV_DRAW_BUF_ALIGN requirement on
/// all platforms (x86_64 defaults byte arrays to 1-byte alignment).
#[repr(align(4))]
#[allow(dead_code)]
struct BandBuf([u8; BAND_BUF_SIZE]);
static mut BAND_BUF: BandBuf = BandBuf([0u8; BAND_BUF_SIZE]);

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize LVGL, the ST7789 display, and the XPT2046 touch controller.
///
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);

    // Initialize hardware
    hal::display::init();
    hal::touch::init();
    hal::display::set_backlight(true);

    unsafe {
        // Initialize LVGL core
        lv_init();

        // Create display (320x240)
        let disp = lv_display_create(hal::display::WIDTH as i32, hal::display::HEIGHT as i32);
        lv_display_set_flush_cb(disp, Some(flush_cb));
        lv_display_set_buffers(
            disp,
            core::ptr::addr_of_mut!(BAND_BUF).cast::<u8>() as *mut c_void,
            core::ptr::null_mut(), // single buffer (no double-buffering)
            BAND_BUF_SIZE as u32,
            LV_DISPLAY_RENDER_MODE_PARTIAL,
        );

        // Create touch input device
        let indev = lv_indev_create();
        lv_indev_set_type(indev, LV_INDEV_TYPE_POINTER);
        lv_indev_set_read_cb(indev, Some(touch_read_cb));
        // Scroll threshold is board-specific: resistive touchscreens need
        // a higher value to compensate for ADC jitter.
        lv_indev_set_scroll_limit(indev, hal::display::SCROLL_LIMIT);

        // Keypad indev — interrupt-driven via GPIO ISR queue
        let keypad = lv_indev_create();
        lv_indev_set_type(keypad, LV_INDEV_TYPE_KEYPAD);
        lv_indev_set_read_cb(keypad, Some(keypad_read_cb));

        // Default group — focusable widgets are navigable via buttons
        let group = lv_group_create();
        lv_group_set_default(group);
        lv_indev_set_group(keypad, group);
    }

    // Init GPIO edge interrupts for hardware buttons (Enviro+ A/B/X/Y)
    init_button_pins();
}

/// Advance LVGL's internal tick counter and process pending timers/rendering.
///
/// Call this periodically (e.g. every 16 ms for ~60 fps).
pub fn tick(ms: u32) {
    unsafe {
        lv_tick_inc(ms);
        lv_timer_handler();
    }
}

/// Get the active screen object.
pub fn screen() -> *mut lv_obj_t {
    unsafe { lv_screen_active() }
}

/// Re-export calibration entry point so existing `engine::calibrate()` calls
/// continue to work without changes.
pub use super::calibration::calibrate;

/// LVGL display flush callback — called by LVGL when a region is ready to send.
///
/// # Safety
/// Called from LVGL's internal rendering pipeline.
unsafe extern "C" fn flush_cb(disp: *mut lv_display_t, area: *const lv_area_t, px_map: *mut u8) {
    let area = &*area;
    let x1 = area.x1 as u16;
    let y1 = area.y1 as u16;
    let x2 = area.x2 as u16;
    let y2 = area.y2 as u16;

    // Set the display window and stream the pixel data
    hal::display::set_window(x1, y1, x2, y2);

    let w = (x2 - x1 + 1) as usize;
    let h = (y2 - y1 + 1) as usize;
    let byte_count = w * h * 2; // RGB565 = 2 bytes per pixel
    let data = core::slice::from_raw_parts(px_map, byte_count);
    hal::display::write_pixels(data);

    // Tell LVGL the flush is complete
    lv_display_flush_ready(disp);
}

/// Track previous touch state so we can detect the released→pressed transition
/// and discard the first (unsettled) ADC reading from the XPT2046.
static mut TOUCH_WAS_PRESSED: bool = false;

/// LVGL input device read callback — called by LVGL to poll touch state.
///
/// # Safety
/// Called from LVGL's internal input processing pipeline.
unsafe extern "C" fn touch_read_cb(_indev: *mut lv_indev_t, data: *mut lv_indev_data_t) {
    let data = &mut *data;
    match hal::touch::read_point() {
        Some((x, y)) => {
            if !TOUCH_WAS_PRESSED {
                // First reading after touch-down: the XPT2046 resistive
                // panel's RC network hasn't settled yet, so this sample
                // can be 20-60 px off.  Discard it — report as still
                // released so LVGL ignores the coordinates.
                TOUCH_WAS_PRESSED = true;
                data.state = LV_INDEV_STATE_RELEASED;
            } else {
                data.point.x = x as i32;
                data.point.y = y as i32;
                data.state = LV_INDEV_STATE_PRESSED;
            }
        }
        None => {
            TOUCH_WAS_PRESSED = false;
            data.state = LV_INDEV_STATE_RELEASED;
        }
    }
    data.continue_reading = false;
}

// ── Keypad input (interrupt-driven via GPIO ISR queue) ───────────────────────

const BUTTON_PINS: &[(u8, u32)] = &[
    (12, LV_KEY_PREV),
    (13, LV_KEY_NEXT),
    (14, LV_KEY_ENTER),
    (15, LV_KEY_ESC),
];

/// Android keycode for each hardware button pin. Parallel to `BUTTON_PINS`.
/// Semantically aligned: PREV→UP, NEXT→DOWN, ENTER→CENTER, ESC→BACK.
const PIN_TO_ANDROID_KEYCODE: &[(u8, i32)] = &[
    (12, 19), // KEYCODE_DPAD_UP
    (13, 20), // KEYCODE_DPAD_DOWN
    (14, 23), // KEYCODE_DPAD_CENTER
    (15, 4),  // KEYCODE_BACK
];

/// Look up the Android keycode for a hardware button pin.
pub fn pin_to_keycode(pin: u8) -> Option<i32> {
    PIN_TO_ANDROID_KEYCODE
        .iter()
        .find(|&&(p, _)| p == pin)
        .map(|&(_, k)| k)
}

fn init_button_pins() {
    for &(pin, _) in BUTTON_PINS {
        hal::gpio::set_input(pin, hal::gpio::Pull::Up);
        hal::gpio::enable_edge_irq(pin, hal::gpio::EdgeTrigger::Both);
    }
    hal::gpio::init_gpio_irq();
}

// ── Java-visible key event queue (parallel to LVGL's internal queue) ─────────

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

/// Return the Java `View` object reference for LVGL's currently focused widget,
/// if one is registered as a key listener via `view::register_key_listener`.
///
/// Returns `None` when there is no default group, no focused widget, or the
/// focused widget has never had `setOnKeyListener` called on it.
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

unsafe extern "C" fn keypad_read_cb(_indev: *mut lv_indev_t, data: *mut lv_indev_data_t) {
    let d = &mut *data;
    if let Some(event) = hal::gpio::drain_gpio_event() {
        // Mirror the event onto the Java-visible queue before LVGL consumes
        // it for focus navigation. The two paths are independent from here.
        push_key_event_raw(event.pin, event.rising);

        let key = BUTTON_PINS
            .iter()
            .find(|&&(p, _)| p == event.pin)
            .map(|&(_, k)| k);
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

    #[test]
    fn pin_to_keycode_maps_known_pins() {
        assert_eq!(pin_to_keycode(12), Some(19)); // UP
        assert_eq!(pin_to_keycode(13), Some(20)); // DOWN
        assert_eq!(pin_to_keycode(14), Some(23)); // CENTER
        assert_eq!(pin_to_keycode(15), Some(4)); // BACK
    }

    #[test]
    fn pin_to_keycode_returns_none_for_unmapped() {
        assert_eq!(pin_to_keycode(0), None);
        assert_eq!(pin_to_keycode(11), None);
        assert_eq!(pin_to_keycode(16), None);
    }

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

    #[test]
    fn key_event_queue_wraps_around() {
        reset_key_event_queue();
        // Fill and drain multiple times to exercise wrap-around.
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
