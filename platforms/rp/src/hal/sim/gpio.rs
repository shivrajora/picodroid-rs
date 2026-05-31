// SPDX-License-Identifier: GPL-3.0-only
use core::sync::atomic::{AtomicU32, Ordering};
use std::collections::VecDeque;
use std::sync::Mutex;

static GPIO_OE: AtomicU32 = AtomicU32::new(0);
static GPIO_OUT: AtomicU32 = AtomicU32::new(0);
static GPIO_IN: AtomicU32 = AtomicU32::new(0xFFFF_FFFF); // default: all high (pull-up)

pub fn set_direction(pin: u8, direction: i32) {
    GPIO_OE.fetch_or(1u32 << pin, Ordering::Relaxed);
    if direction == 1 {
        GPIO_OUT.fetch_or(1u32 << pin, Ordering::Relaxed);
        println!("[sim] GP{pin}: output, initially HIGH");
    } else {
        GPIO_OUT.fetch_and(!(1u32 << pin), Ordering::Relaxed);
        println!("[sim] GP{pin}: output, initially LOW");
    }
}

pub fn set_value(pin: u8, high: bool) {
    set_value_silent(pin, high);
    println!("[sim] GP{pin}: {}", if high { "HIGH" } else { "LOW" });
}

/// Like `set_value` but without the println. Used by driver-level pin
/// toggles (e.g. SPI CS bit-bang) that fire at LVGL tick rate and would
/// otherwise spam the log.
pub fn set_value_silent(pin: u8, high: bool) {
    if high {
        GPIO_OUT.fetch_or(1u32 << pin, Ordering::Relaxed);
    } else {
        GPIO_OUT.fetch_and(!(1u32 << pin), Ordering::Relaxed);
    }
}

// ── Input ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub enum Pull {
    None,
    Up,
    Down,
}

pub fn set_input(pin: u8, pull: Pull) {
    GPIO_OE.fetch_and(!(1u32 << pin), Ordering::Relaxed);
    match pull {
        Pull::Up => GPIO_IN.fetch_or(1u32 << pin, Ordering::Relaxed),
        Pull::Down => GPIO_IN.fetch_and(!(1u32 << pin), Ordering::Relaxed),
        Pull::None => GPIO_IN.fetch_and(!(1u32 << pin), Ordering::Relaxed),
    };
    println!(
        "[sim] GP{pin}: input, pull={}",
        match pull {
            Pull::Up => "UP",
            Pull::Down => "DOWN",
            Pull::None => "NONE",
        }
    );
}

pub fn read(pin: u8) -> bool {
    (GPIO_IN.load(Ordering::Relaxed) >> pin) & 1 != 0
}

// ── Edge interrupt stubs ─────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub enum EdgeTrigger {
    Rising,
    Falling,
    Both,
}

pub fn enable_edge_irq(_pin: u8, _edge: EdgeTrigger) {}
pub fn disable_edge_irq(_pin: u8) {}
pub fn init_gpio_irq() {}

#[derive(Clone, Copy)]
pub struct GpioEvent {
    pub pin: u8,
    pub rising: bool,
}

/// Synthetic button-edge queue. On hardware these edges come from the GPIO
/// IRQ; the host sim has no GPIO peripheral, so the input front-ends in
/// `hal::sim::display` (keyboard + control channel) call [`inject`] instead.
/// A `Mutex` (rather than the `static mut` used elsewhere in this file)
/// because the control-channel front-end injects from a background thread.
static GPIO_EVENTS: Mutex<VecDeque<GpioEvent>> = Mutex::new(VecDeque::new());

/// Enqueue a synthetic button edge. Active-low convention: `rising == false`
/// is a PRESS (falling edge → `LV_INDEV_STATE_PRESSED`), `rising == true` a
/// RELEASE. Always inject a PRESS before its matching RELEASE for a given pin,
/// or the phantom-release filter in `lvgl::events` drops the unpaired release.
pub fn inject(pin: u8, rising: bool) {
    GPIO_EVENTS
        .lock()
        .unwrap()
        .push_back(GpioEvent { pin, rising });
}

pub fn drain_gpio_event() -> Option<GpioEvent> {
    GPIO_EVENTS.lock().unwrap().pop_front()
}

pub fn has_pending_event() -> bool {
    !GPIO_EVENTS.lock().unwrap().is_empty()
}

/// No-op in sim — the host has no GPIO IRQ to block on.
pub fn wait_for_button_event() {}
