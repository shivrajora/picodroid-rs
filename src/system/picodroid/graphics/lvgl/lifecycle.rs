//! LVGL lifecycle — `lv_init`, display + touch indev creation, tick, sleep,
//! wake, screen access, and the partial-render band buffer.
//!
//! Owns the only RGB565 `BAND_BUF` static (size derived from `hal::display`
//! constants — board.toml-driven). Keypad-specific lifecycle (the keypad
//! indev, focus group, button GPIO pins) lives in `lvgl::events` (step 5
//! of the plan).

use crate::hal;
use crate::lvgl_ffi::*;
use crate::system::picodroid::graphics::gfx::Handle;
use crate::system::picodroid::graphics::handle_table;
use core::ffi::c_void;

// ── Band buffer (RGB565 partial render scratch) ─────────────────────────────

const BAND_HEIGHT: usize = hal::display::BAND_HEIGHT;
const BAND_BUF_SIZE: usize = hal::display::WIDTH as usize * BAND_HEIGHT * 2;

/// Wrapper to get a raw pointer without creating a mutable reference.
/// Must be 4-byte aligned to satisfy LVGL's `LV_DRAW_BUF_ALIGN` requirement
/// on all platforms (x86_64 defaults byte arrays to 1-byte alignment).
#[repr(align(4))]
#[allow(dead_code)] // field accessed only via raw pointer (LVGL flush callback)
struct BandBuf([u8; BAND_BUF_SIZE]);
static mut BAND_BUF: BandBuf = BandBuf([0u8; BAND_BUF_SIZE]);

// ── Screen handle cache ─────────────────────────────────────────────────────

/// Handle table id of the active screen, set during `init`. The active
/// screen pointer is stable across the program's lifetime in our usage
/// (we never call `lv_screen_load`), so caching once is safe.
static mut SCREEN_HANDLE: Handle = Handle::NULL;

// ── Public lifecycle entry points (called from LvglGfx trait impl) ──────────

/// LVGL init — idempotency is the caller's responsibility (today: gated by
/// `engine::init`'s `INITIALIZED` flag).
pub(in crate::system::picodroid::graphics) fn init(width: u16, height: u16) {
    hal::display::init();
    hal::touch::init();
    hal::display::set_backlight(true);

    unsafe {
        lv_init();

        let disp = lv_display_create(width as i32, height as i32);
        lv_display_set_flush_cb(disp, Some(flush_cb));
        lv_display_set_buffers(
            disp,
            core::ptr::addr_of_mut!(BAND_BUF).cast::<u8>() as *mut c_void,
            core::ptr::null_mut(), // single buffer (no double-buffering)
            BAND_BUF_SIZE as u32,
            LV_DISPLAY_RENDER_MODE_PARTIAL,
        );

        let indev = lv_indev_create();
        lv_indev_set_type(indev, LV_INDEV_TYPE_POINTER);
        lv_indev_set_read_cb(indev, Some(touch_read_cb));
        lv_indev_set_scroll_limit(indev, hal::display::SCROLL_LIMIT);

        // Cache a Handle for the screen so `LvglGfx::screen()` can return
        // a backend-neutral type. The screen pointer is stable post-init.
        let scr = lv_screen_active();
        SCREEN_HANDLE = Handle::from_java(handle_table::register(scr));
    }
}

pub(in crate::system::picodroid::graphics) fn tick(ms: u32) {
    unsafe {
        lv_tick_inc(ms);
        lv_timer_handler();
    }
}

pub(in crate::system::picodroid::graphics) fn sleep() {
    hal::display::display_sleep();
}

pub(in crate::system::picodroid::graphics) fn wake() {
    hal::display::display_wake();
    unsafe {
        let scr = lv_screen_active();
        if !scr.is_null() {
            lv_obj_invalidate(scr);
        }
    }
}

pub(in crate::system::picodroid::graphics) fn screen_handle() -> Handle {
    unsafe { SCREEN_HANDLE }
}

/// Raw screen pointer accessor for legacy callers (widgets that still use
/// `engine::screen()` pre-step-7 migration). Goes away when widgets
/// migrate to `with_gfx(|g| g.screen())`.
pub(in crate::system::picodroid::graphics) fn screen_ptr() -> *mut lv_obj_t {
    unsafe { lv_screen_active() }
}

// ── Display flush callback ──────────────────────────────────────────────────

/// LVGL display flush callback — called by LVGL when a region is ready to
/// send.
///
/// # Safety
/// Called from LVGL's internal rendering pipeline.
unsafe extern "C" fn flush_cb(disp: *mut lv_display_t, area: *const lv_area_t, px_map: *mut u8) {
    let area = unsafe { &*area };
    let x1 = area.x1 as u16;
    let y1 = area.y1 as u16;
    let x2 = area.x2 as u16;
    let y2 = area.y2 as u16;

    hal::display::set_window(x1, y1, x2, y2);

    let w = (x2 - x1 + 1) as usize;
    let h = (y2 - y1 + 1) as usize;
    let byte_count = w * h * 2; // RGB565 = 2 bytes per pixel
    let data = unsafe { core::slice::from_raw_parts(px_map, byte_count) };
    hal::display::write_pixels(data);

    unsafe { lv_display_flush_ready(disp) };
}

// ── Touch read callback ─────────────────────────────────────────────────────

/// Track previous touch state so we can detect the released→pressed
/// transition and discard the first (unsettled) ADC reading from the
/// XPT2046.
static mut TOUCH_WAS_PRESSED: bool = false;

/// LVGL input device read callback — called by LVGL to poll touch state.
///
/// # Safety
/// Called from LVGL's internal input processing pipeline.
unsafe extern "C" fn touch_read_cb(_indev: *mut lv_indev_t, data: *mut lv_indev_data_t) {
    let data = unsafe { &mut *data };
    match hal::touch::read_point() {
        Some((x, y)) => {
            // SAFETY: single-threaded LVGL callback; only this fn touches the
            // flag.
            let was_pressed = unsafe { TOUCH_WAS_PRESSED };
            if !was_pressed {
                // First reading after touch-down: the XPT2046 resistive
                // panel's RC network hasn't settled yet, so this sample
                // can be 20-60 px off. Discard it — report as still
                // released so LVGL ignores the coordinates.
                unsafe { TOUCH_WAS_PRESSED = true };
                data.state = LV_INDEV_STATE_RELEASED;
            } else {
                data.point.x = x as i32;
                data.point.y = y as i32;
                data.state = LV_INDEV_STATE_PRESSED;
            }
        }
        None => {
            unsafe { TOUCH_WAS_PRESSED = false };
            data.state = LV_INDEV_STATE_RELEASED;
        }
    }
    data.continue_reading = false;
}
