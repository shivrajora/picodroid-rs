// SPDX-License-Identifier: GPL-3.0-only
//! LVGL lifecycle — `lv_init`, display + touch indev creation, tick, sleep,
//! wake, screen access, and the partial-render band buffer.
//!
//! Owns the only RGB565 `BAND_BUF` static (size derived from `hal::display`
//! constants — board.toml-driven). Keypad-specific lifecycle (the keypad
//! indev, focus group, button GPIO pins) lives in `lvgl::events` (step 5
//! of the plan).
//!
//! # The flush is asynchronous
//!
//! `flush_cb` starts a band's transfer and returns; LVGL collects it through
//! `flush_wait_cb` — before it renders into that buffer again, or, with two
//! buffers (board.toml `draw_buffers = 2`), before it hands the next one
//! over. So on a family whose transfer is DMA the render of band *n+1*
//! overlaps the transfer of band *n*, and the last band of a frame drains
//! while the UI task goes back to Java. The panel driver finishes any
//! transfer in flight before it sends a command, which is what keeps a
//! `VSCRSADD`, a `set_window` or a sleep from cutting a band short
//! (docs/designs/scroll-performance-2026-09.md S5).

use crate::graphics::gfx::Handle;
use crate::hal;
use crate::lvgl_ffi::*;
use crate::util::local::Core0;
use core::cell::Cell;
use core::ffi::c_void;

use super::handle_table;

// ── Band buffer (RGB565 partial render scratch) ─────────────────────────────

const BAND_HEIGHT: usize = hal::display::BAND_HEIGHT;
const BAND_BUF_SIZE: usize = hal::display::WIDTH as usize * BAND_HEIGHT * 2;
/// One or two: with two, LVGL renders into one band while the other is
/// still on its way to the panel. Both are the same size, back to back in
/// the one static, so the second is `BAND_BUF_SIZE` past the first.
const DRAW_BUFFERS: usize = hal::display::DRAW_BUFFERS;

/// Wrapper to get a raw pointer without creating a mutable reference.
/// Must be 4-byte aligned to satisfy LVGL's `LV_DRAW_BUF_ALIGN` requirement
/// on all platforms (x86_64 defaults byte arrays to 1-byte alignment).
#[repr(align(4))]
#[allow(dead_code)] // field accessed only via raw pointer (LVGL flush callback)
struct BandBuf([u8; BAND_BUF_SIZE * DRAW_BUFFERS]);
static mut BAND_BUF: BandBuf = BandBuf([0u8; BAND_BUF_SIZE * DRAW_BUFFERS]);

// ── Screen handle cache ─────────────────────────────────────────────────────

/// Handle table id of the active screen, set during `init`. The active
/// screen pointer is stable across the program's lifetime in our usage
/// (we never call `lv_screen_load`), so caching once is safe.
// SAFETY: widget-layer state, reached only from JVM tasks.
static SCREEN_HANDLE: Core0<Cell<Handle>> = unsafe { Core0::new(Cell::new(Handle::NULL)) };

// ── Public lifecycle entry points (called from LvglGfx trait impl) ──────────

/// LVGL init — idempotency is the caller's responsibility (today: gated by
/// `engine::init`'s `INITIALIZED` flag).
pub(in crate::graphics) fn init(width: u16, height: u16) {
    hal::display::init();
    hal::touch::init();
    // Off the UI task from here on, where the panel's bus allows it: a frame
    // on the touch board costs 120-200 ms, and sampling a finger at that rate
    // is what made scrolling teleport. No-op on every other board.
    hal::touch_sampler::start();
    hal::display::set_backlight(true);

    unsafe {
        lv_init();
        // A pool in PSRAM holds what LVGL keeps between frames; the layers
        // it renders into are written per pixel and come from SRAM instead
        // (lvgl/lv_draw_buf_sram.c).
        #[cfg(lv_mem_in_psram)]
        picodroid_lv_draw_buf_use_sram();

        let disp = lv_display_create(width as i32, height as i32);
        lv_display_set_flush_cb(disp, Some(flush_cb));
        lv_display_set_flush_wait_cb(disp, Some(flush_wait_cb));
        let buf1 = core::ptr::addr_of_mut!(BAND_BUF).cast::<u8>();
        let buf2 = if DRAW_BUFFERS == 2 {
            buf1.add(BAND_BUF_SIZE)
        } else {
            core::ptr::null_mut()
        };
        lv_display_set_buffers(
            disp,
            buf1 as *mut c_void,
            buf2 as *mut c_void,
            BAND_BUF_SIZE as u32,
            LV_DISPLAY_RENDER_MODE_PARTIAL,
        );
        // A panel that can scroll its own frame memory: hook the display's
        // invalidation and refresh events so a scroll step renders only the
        // rows it exposed (hw_scroll.rs).
        #[cfg(hw_vscroll)]
        super::hw_scroll::install(disp);

        let indev = lv_indev_create();
        lv_indev_set_type(indev, LV_INDEV_TYPE_POINTER);
        lv_indev_set_read_cb(indev, Some(touch_read_cb));
        lv_indev_set_scroll_limit(indev, hal::display::SCROLL_LIMIT);
        POINTER_INDEV.set(indev);

        // Cache a Handle for the screen so `LvglGfx::screen()` can return
        // a backend-neutral type. The screen pointer is stable post-init.
        // Pinned: the screen is never deleted, must survive the between-app
        // handle_table::reset() (PDB reload), and must not accumulate
        // LV_EVENT_DELETE hooks — register_pinned covers all three.
        let scr = lv_screen_active();
        SCREEN_HANDLE.set(Handle::from_java(handle_table::register_pinned(scr)));
    }
    // The app may have declared a design size before the display existed
    // (`run_app` runs first on a cold boot): the window takes effect now.
    super::window::sync();
    // A board with no system button gets its BACK/HOME control on the top
    // layer now, above every screen an app will load.
    super::soft_nav::ensure();
    super::menu_button::ensure();
}

pub(in crate::graphics) fn tick(ms: u32) {
    unsafe {
        lv_tick_inc(ms);
        lv_timer_handler();
    }
    // A draw buffer the arena could not serve went to the PSRAM pool and
    // was rendered through the QSPI bus: say so once, so an unexplained
    // slow frame has a line in the log rather than a mystery.
    #[cfg(lv_mem_in_psram)]
    {
        use core::sync::atomic::{AtomicBool, Ordering};
        static WARNED: AtomicBool = AtomicBool::new(false);
        let n = unsafe { picodroid_lv_draw_buf_pool_fallbacks() };
        if n > 0 && !WARNED.swap(true, Ordering::Relaxed) {
            defmt::warn!(
                "lvgl: {=u32} draw buffer(s) fell back to the PSRAM pool (arena full)",
                n
            );
        }
    }
}

/// The pointer input device, kept so a wake can tell it to sit out the
/// press that woke the panel, and so the soft keyboard can listen for a
/// press anywhere on it.
// SAFETY: widget-layer state, reached only from JVM tasks.
static POINTER_INDEV: Core0<Cell<*mut lv_indev_t>> =
    unsafe { Core0::new(Cell::new(core::ptr::null_mut())) };

/// The pointer input device: created in `init`, never deleted; null before.
pub(in crate::graphics) fn pointer_indev() -> *mut lv_indev_t {
    POINTER_INDEV.get()
}

pub(in crate::graphics) fn sleep() {
    hal::display::display_sleep();
}

/// What woke the panel must not act on what it finds there: drop the queued
/// key edges (their releases are dropped by the press-state filter), drain
/// the touch ring, and have LVGL ignore the current press until it lifts.
fn swallow_wake_input() {
    while hal::gpio::drain_gpio_event().is_some() {}
    crate::input_inject::reset_soft_keys();
    while hal::touch_sampler::next().is_some() {}
    LAST_DELIVERED.set(None);
    let indev = POINTER_INDEV.get();
    if !indev.is_null() {
        // SAFETY: the pointer indev is created in `init` and never deleted.
        unsafe { lv_indev_wait_release(indev) };
    }
}

pub(in crate::graphics) fn wake() {
    swallow_wake_input();
    hal::display::display_wake();
    // The full repaint below goes out through the identity rotation; make
    // sure the panel is told so before the first band, whatever sleep did
    // to its registers.
    #[cfg(hw_vscroll)]
    super::hw_scroll::panel_reset();
    unsafe {
        let scr = lv_screen_active();
        if !scr.is_null() {
            lv_obj_invalidate(scr);
        }
    }
}

/// Where content roots go: the screen, or the compat window's object while
/// an app with a design size runs (`window.rs`).
pub(in crate::graphics) fn screen_handle() -> Handle {
    let content = CONTENT_HANDLE.get();
    if content.is_null() {
        SCREEN_HANDLE.get()
    } else {
        content
    }
}

/// The compat window's object (registered for the Java side) as the content
/// parent, or the screen again when `pinned` is false and `obj` is the
/// screen. Called by `window::sync` only.
pub(super) fn set_content_parent(obj: *mut lv_obj_t, windowed: bool) {
    CONTENT_HANDLE.set(if windowed {
        Handle::from_java(handle_table::register(obj))
    } else {
        Handle::NULL
    });
}

/// The handle `screen_handle` answers while a compat window is up, null
/// otherwise (the pinned screen handle then).
// SAFETY: widget-layer state, reached only from JVM tasks.
static CONTENT_HANDLE: Core0<Cell<Handle>> = unsafe { Core0::new(Cell::new(Handle::NULL)) };

/// Raw pointer every widget is created under before `addView` re-parents
/// it: the screen, or the compat window's object (`window.rs`).
pub(in crate::graphics) fn screen_ptr() -> *mut lv_obj_t {
    super::window::content_root()
}

/// Where a system overlay is parented: LVGL's top layer, above every screen.
/// An app's content root lives on the screen, which pans when the content is
/// larger than the window (docs/designs/app-portability-2026-10.md D6); a
/// toast, snackbar, dialog or the soft keyboard must not pan with it, and must
/// outlive a `setContentView` that replaces the root.
pub(in crate::graphics) fn overlay_layer() -> *mut lv_obj_t {
    // SAFETY: LVGL is initialised before any widget code runs; the top layer
    // is created with the display and lives as long as it does.
    unsafe { lv_layer_top() }
}

/// The window an app and the overlays are laid out in: the display's logical
/// size. The panel today; an app run in a compat window (D5) gets its design
/// size, which is why overlays size themselves from here and not from the
/// `hal::display` panel constants.
pub(in crate::graphics) fn window_size() -> (i32, i32) {
    // SAFETY: the default display exists from `init` on and is never deleted;
    // the getters only read it.
    unsafe {
        let disp = lv_display_get_default();
        (
            lv_display_get_horizontal_resolution(disp),
            lv_display_get_vertical_resolution(disp),
        )
    }
}

// ── Display flush callback ──────────────────────────────────────────────────

/// Flushed-band counters for the parity harness (docs/parity-audit.md P1):
/// deterministic render work, asserted equal between sim and device.
#[cfg(feature = "parity-metrics")]
pub mod flush_stats {
    use core::sync::atomic::{AtomicUsize, Ordering};
    pub static BANDS: AtomicUsize = AtomicUsize::new(0);
    pub static BYTES: AtomicUsize = AtomicUsize::new(0);
    pub fn snapshot() -> (usize, usize) {
        (BANDS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed))
    }
}

/// CRC32 (IEEE, bitwise) of a flushed band. Table-free so the device build
/// pays no flash for it; ~1-2 ms per 12.8 KB band on an M33 — acceptable in
/// `parity-fbhash` test builds, which exist only for sequence comparison.
#[cfg(feature = "parity-fbhash")]
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// LVGL display flush callback — called by LVGL when a region is ready to
/// send. Starts the transfer and returns; [`flush_wait_cb`] is where LVGL
/// waits for it, so `lv_display_flush_ready` is never called here.
///
/// # Safety
/// Called from LVGL's internal rendering pipeline.
unsafe extern "C" fn flush_cb(_disp: *mut lv_display_t, area: *const lv_area_t, px_map: *mut u8) {
    let area = unsafe { &*area };
    let x1 = area.x1 as u16;
    let y1 = area.y1 as u16;
    let x2 = area.x2 as u16;
    let y2 = area.y2 as u16;

    #[cfg(not(hw_vscroll))]
    hal::display::set_window(x1, y1, x2, y2);

    let w = (x2 - x1 + 1) as usize;
    let h = (y2 - y1 + 1) as usize;
    let byte_count = w * h * 2; // RGB565 = 2 bytes per pixel
    let data = unsafe { core::slice::from_raw_parts(px_map, byte_count) };

    #[cfg(feature = "parity-metrics")]
    {
        use core::sync::atomic::Ordering;
        flush_stats::BANDS.fetch_add(1, Ordering::Relaxed);
        flush_stats::BYTES.fetch_add(byte_count, Ordering::Relaxed);
    }
    // G1: hash the BE-RGB565 band bytes at the shared seam — before the
    // sim's ARGB conversion — so sim and device hash identical data by
    // construction (docs/parity-audit.md DSP-01/G1).
    #[cfg(feature = "parity-fbhash")]
    {
        let crc = crc32(data);
        #[cfg(not(feature = "sim"))]
        defmt::info!(
            "fbhash: {=u16},{=u16},{=u16},{=u16} {=u32:08x}",
            x1,
            y1,
            x2,
            y2,
            crc
        );
        #[cfg(feature = "sim")]
        println!("fbhash: {},{},{},{} {:08x}", x1, y1, x2, y2, crc);
    }

    // Through the panel's current rotation where it has one: a display row
    // and the memory line that shows it are the same thing only until the
    // first hardware scroll step.
    #[cfg(hw_vscroll)]
    super::hw_scroll::flush(x1, y1, x2, y2, data);
    #[cfg(not(hw_vscroll))]
    hal::display::write_pixels_start(data);
}

/// LVGL's wait for the last flush: before it renders into a buffer that is
/// still being sent, and — with two buffers — before it starts the next
/// transfer. LVGL clears its own `flushing` flag when this returns.
///
/// # Safety
/// Called from LVGL's internal rendering pipeline.
unsafe extern "C" fn flush_wait_cb(_disp: *mut lv_display_t) {
    hal::display::write_pixels_wait();
}

// ── Touch read callback ─────────────────────────────────────────────────────

/// The last sample handed to LVGL. When the sampler's ring is empty — an
/// unmoving finger, or no finger — LVGL still asks on every read and has to be
/// told the state it is already in, or a held press would look like a lift.
// SAFETY: widget-layer state, reached only from JVM tasks.
static LAST_DELIVERED: Core0<Cell<Option<(u16, u16)>>> = unsafe { Core0::new(Cell::new(None)) };

/// LVGL input device read callback — called by LVGL to poll touch state.
///
/// Where [`hal::touch_sampler`] owns the panel this drains its ring, one
/// queued sample per call, asking LVGL to read again while more remain
/// (`continue_reading`). LVGL then processes every position the finger
/// actually passed through, in one pass, and renders once at the end: the
/// motion it sees is a drag rather than the teleport a once-per-frame sample
/// produced. Elsewhere — a panel sharing the display's bus — it reads the
/// panel inline, as it always did.
///
/// # Safety
/// Called from LVGL's internal input processing pipeline.
unsafe extern "C" fn touch_read_cb(_indev: *mut lv_indev_t, data: *mut lv_indev_data_t) {
    let data = unsafe { &mut *data };

    let (sample, more) = if hal::touch_sampler::running() {
        match hal::touch_sampler::next() {
            Some(s) => (s, hal::touch_sampler::pending()),
            // SAFETY: single-threaded LVGL callback; only this fn touches it.
            None => (LAST_DELIVERED.get(), false),
        }
    } else {
        (hal::touch_sampler::sample_panel(), false)
    };
    LAST_DELIVERED.set(sample);

    match sample {
        Some((x, y)) => {
            data.point.x = x as i32;
            data.point.y = y as i32;
            data.state = LV_INDEV_STATE_PRESSED;
            // A finger on the glass: the panel stays on (power.rs).
            crate::power::user_activity();
        }
        None => data.state = LV_INDEV_STATE_RELEASED,
    }
    data.continue_reading = more;
}
