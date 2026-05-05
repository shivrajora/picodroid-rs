// SPDX-License-Identifier: GPL-3.0-only
use core::sync::atomic::{AtomicU32, Ordering};

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
    if high {
        GPIO_OUT.fetch_or(1u32 << pin, Ordering::Relaxed);
    } else {
        GPIO_OUT.fetch_and(!(1u32 << pin), Ordering::Relaxed);
    }
    println!("[sim] GP{pin}: {}", if high { "HIGH" } else { "LOW" });
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

pub fn drain_gpio_event() -> Option<GpioEvent> {
    None
}

pub fn has_pending_event() -> bool {
    false
}

/// No-op in sim — the host has no GPIO IRQ to block on.
pub fn wait_for_button_event() {}
