//! Simulator display backend using minifb to render LVGL output in a desktop
//! window.  Replaces the previous no-op stubs so that graphical apps (e.g.
//! `displaydemo`) can be tested without hardware.

use minifb::{Key, MouseButton, MouseMode, Scale, ScaleMode, Window, WindowOptions};

// Board-conditional display constants (duplicated from board configs because
// board modules are not compiled for the simulator target).
#[cfg(feature = "board-testbench")]
pub const WIDTH: u16 = 320;
#[cfg(feature = "board-testbench")]
pub const HEIGHT: u16 = 240;
#[cfg(feature = "board-testbench")]
pub const BAND_HEIGHT: usize = 20;
#[cfg(feature = "board-testbench")]
pub const SCROLL_LIMIT: u8 = 30;

#[cfg(feature = "board-pico-enviro-mon")]
pub const WIDTH: u16 = 240;
#[cfg(feature = "board-pico-enviro-mon")]
pub const HEIGHT: u16 = 135;
#[cfg(feature = "board-pico-enviro-mon")]
pub const BAND_HEIGHT: usize = 27;
#[cfg(feature = "board-pico-enviro-mon")]
pub const SCROLL_LIMIT: u8 = 10;

// Fallback when no board feature is active (e.g. plain `cargo test`)
#[cfg(not(any(feature = "board-testbench", feature = "board-pico-enviro-mon")))]
pub const WIDTH: u16 = 320;
#[cfg(not(any(feature = "board-testbench", feature = "board-pico-enviro-mon")))]
pub const HEIGHT: u16 = 240;
#[cfg(not(any(feature = "board-testbench", feature = "board-pico-enviro-mon")))]
pub const BAND_HEIGHT: usize = 20;
#[cfg(not(any(feature = "board-testbench", feature = "board-pico-enviro-mon")))]
pub const SCROLL_LIMIT: u8 = 30;

const NUM_PIXELS: usize = WIDTH as usize * HEIGHT as usize;

// ── Statics (single-threaded sim — safe) ────────────────────────────────────

static mut WINDOW: Option<Window> = None;
static mut FRAMEBUF: [u32; NUM_PIXELS] = [0u32; NUM_PIXELS];

/// Current draw region set by `set_window()`.
static mut WIN_X0: u16 = 0;
static mut WIN_Y0: u16 = 0;
static mut WIN_X1: u16 = 0;
static mut WIN_Y1: u16 = 0;

/// Mouse state sampled during `update_window()`.
static mut MOUSE_PRESSED: bool = false;
static mut MOUSE_X: u16 = 0;
static mut MOUSE_Y: u16 = 0;

// ── Public API (matches hal::display contract) ──────────────────────────────

pub fn init() {
    let opts = WindowOptions {
        scale: Scale::X2,
        scale_mode: ScaleMode::AspectRatioStretch,
        ..WindowOptions::default()
    };

    let mut win =
        Window::new("picodroid", WIDTH as usize, HEIGHT as usize, opts).expect("minifb window");

    // Limit update rate to ~60 fps
    win.set_target_fps(60);

    unsafe {
        WINDOW = Some(win);
    }
}

pub fn set_window(x0: u16, y0: u16, x1: u16, y1: u16) {
    unsafe {
        WIN_X0 = x0;
        WIN_Y0 = y0;
        WIN_X1 = x1;
        WIN_Y1 = y1;
    }
}

/// Write byte-swapped RGB565 pixel data into the framebuffer at the region
/// previously set by `set_window()`.
pub fn write_pixels(data: &[u8]) {
    unsafe {
        let x0 = WIN_X0 as usize;
        let y0 = WIN_Y0 as usize;
        let x1 = WIN_X1 as usize;
        let _w = x1 - x0 + 1;
        let stride = WIDTH as usize;

        let mut i = 0; // byte index into data
        let mut row = y0;
        let mut col = x0;

        while i + 1 < data.len() {
            // LV_COLOR_16_SWAP=1: bytes arrive big-endian
            let raw = u16::from_be_bytes([data[i], data[i + 1]]);
            let r = ((raw >> 11) & 0x1F) as u32;
            let g = ((raw >> 5) & 0x3F) as u32;
            let b = (raw & 0x1F) as u32;
            let argb = 0xFF00_0000 | (r * 255 / 31) << 16 | (g * 255 / 63) << 8 | (b * 255 / 31);

            FRAMEBUF[row * stride + col] = argb;

            col += 1;
            if col > x1 {
                col = x0;
                row += 1;
            }
            i += 2;
        }
    }
}

pub fn set_backlight(on: bool) {
    println!("[sim] Display: backlight {}", if on { "ON" } else { "OFF" });
}

// ── Emulator-specific functions ─────────────────────────────────────────────

/// Blit the framebuffer to the minifb window and sample mouse state.
///
/// Call once per frame after `engine::tick()`.
pub fn update_window() {
    unsafe {
        if let Some(ref mut win) = WINDOW {
            let ptr = core::ptr::addr_of!(FRAMEBUF) as *const u32;
            let buf = core::slice::from_raw_parts(ptr, NUM_PIXELS);
            win.update_with_buffer(buf, WIDTH as usize, HEIGHT as usize)
                .unwrap_or(());

            // Sample mouse state for touch emulation
            MOUSE_PRESSED = win.get_mouse_down(MouseButton::Left);
            if let Some((mx, my)) = win.get_mouse_pos(MouseMode::Clamp) {
                MOUSE_X = (mx as u16).min(WIDTH - 1);
                MOUSE_Y = (my as u16).min(HEIGHT - 1);
            }
        }
    }
}

/// Returns `true` if the emulator window is still open (user hasn't closed it).
pub fn is_window_open() -> bool {
    unsafe {
        let ptr = &raw const WINDOW;
        match &*ptr {
            Some(win) => win.is_open() && !win.is_key_down(Key::Escape),
            None => true, // no window yet — don't block
        }
    }
}

/// Returns `(pressed, x, y)` — the most recent mouse state sampled by
/// `update_window()`.
pub fn mouse_state() -> (bool, u16, u16) {
    unsafe { (MOUSE_PRESSED, MOUSE_X, MOUSE_Y) }
}
