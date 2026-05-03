// SPDX-License-Identifier: GPL-3.0-only
pub fn set_direction(_pin: u8, _direction: i32) {}
pub fn set_value(_pin: u8, _high: bool) {}
pub fn read(_pin: u8) -> bool {
    true
}

#[derive(Clone, Copy)]
pub enum Pull {
    None,
    Up,
    Down,
}

pub fn set_input(_pin: u8, _pull: Pull) {}

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

pub fn wait_for_button_event() {}
