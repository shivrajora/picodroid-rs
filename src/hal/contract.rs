//! HAL CONTRACT v1 — compile-time assertions.
//!
//! Each `_assert_*` function below is dead code: it exists solely to make the
//! compiler verify that the named symbols and their signatures are present in
//! whichever chip family module is currently active. The bodies type-check but
//! never run.
//!
//! The human-readable contract lives in the doc-block at the top of
//! [`super`]. Keep both in sync — when adding a symbol to one, add it here.

#![allow(dead_code, unused_imports, clippy::let_unit_value)]

use super::{adc, display, gpio, i2c, pwm, spi, system_clock, touch, uart};

fn _assert_always_required() {
    // adc
    let _: fn(u8) = adc::init;
    let _: fn(u8) -> f64 = adc::read;

    // display constants
    let _: u16 = display::WIDTH;
    let _: u16 = display::HEIGHT;
    let _: usize = display::BAND_HEIGHT;
    let _: u8 = display::SCROLL_LIMIT;
    // display functions
    let _: fn() = display::init;
    let _: fn(u16, u16, u16, u16) = display::set_window;
    let _: fn(&[u8]) = display::write_pixels;
    let _: fn(bool) = display::set_backlight;
    let _: fn() = display::display_sleep;
    let _: fn() = display::display_wake;
    let _: fn() = display::update_window;
    let _: fn() -> bool = display::is_window_open;

    // gpio types
    let _: Option<gpio::Pull> = None;
    let _: Option<gpio::EdgeTrigger> = None;
    let _: Option<gpio::GpioEvent> = None;
    // gpio functions
    let _: fn(u8, i32) = gpio::set_direction;
    let _: fn(u8, bool) = gpio::set_value;
    let _: fn(u8, gpio::Pull) = gpio::set_input;
    let _: fn(u8) -> bool = gpio::read;
    let _: fn(u8, gpio::EdgeTrigger) = gpio::enable_edge_irq;
    let _: fn(u8) = gpio::disable_edge_irq;
    let _: fn() = gpio::init_gpio_irq;
    let _: fn() -> Option<gpio::GpioEvent> = gpio::drain_gpio_event;
    let _: fn() -> bool = gpio::has_pending_event;
    let _: fn() = gpio::wait_for_button_event;

    // i2c
    let _: fn(u8) = i2c::init;
    let _: fn(u8, u32) = i2c::set_speed;
    let _: fn(u8, u8, &[u8]) -> i32 = i2c::write_slice;
    let _: fn(u8, u8, &mut [u8]) -> i32 = i2c::read_slice;

    // pwm
    let _: fn(u8) = pwm::init;
    let _: fn(u8, f64, f64, bool) = pwm::apply;

    // spi
    let _: fn(u8) = spi::init;
    let _: fn(u8, u32, u32) = spi::reconfigure;
    let _: fn(u8, &[u8]) = spi::write_raw;
    let _: fn(u8, &[u8], &mut [u8]) = spi::transfer_raw;

    // system_clock
    let _: fn(u32) = system_clock::sleep;
    let _: fn() -> i64 = system_clock::elapsed_realtime_nanos;

    // touch
    let _: fn() = touch::init;
    let _: fn() -> Option<(u16, u16)> = touch::read_point;
    let _: fn() -> (u16, u16) = touch::read_raw_unfiltered;
    let _: fn(u16, u16, u16, u16) = touch::set_calibration;

    // uart
    let _: fn(u8) = uart::init;
    let _: fn(u8, u8) = uart::write_byte;
    let _: fn(u8) -> i32 = uart::read_byte;
}

#[cfg(not(any(test, feature = "sim")))]
fn _assert_hardware_only() {
    use super::{boot, flash, pdb_usb};

    // boot
    let _: fn() = boot::clock_init;
    let _: fn(&'static [u8]) -> ! = boot::start_tasks;

    // flash
    let _: usize = flash::PAPK_MAX_DATA_SIZE;
    let _: unsafe fn() -> Option<&'static [u8]> = flash::read_flash_papk;

    // pdb_usb
    let _: fn() = pdb_usb::init;
    let _: fn() = pdb_usb::drain_tx;
    let _: fn() -> u8 = pdb_usb::queue_read_byte;
    let _: fn() -> Option<u8> = pdb_usb::queue_read_byte_timeout;
    let _: fn() -> u32 = pdb_usb::queue_read_u32_le;
    let _: fn(&[u8]) = pdb_usb::write_bytes;
}

#[cfg(has_network)]
fn _assert_network_required() {
    use core::ffi::c_void;

    use super::net;

    let _: fn() -> Result<*mut c_void, net::NetError> = net::tcp_socket;
    let _: fn(*mut c_void, u32, u16) -> Result<(), net::NetError> = net::tcp_connect;
    let _: fn(*mut c_void, &[u8]) -> Result<usize, net::NetError> = net::tcp_send;
    let _: fn(*mut c_void, &mut [u8]) -> Result<usize, net::NetError> = net::tcp_recv;
    let _: fn(*mut c_void, u16) -> Result<(), net::NetError> = net::tcp_listen;
    let _: fn(*mut c_void) -> Result<*mut c_void, net::NetError> = net::tcp_accept;
    let _: fn(u16) -> Result<*mut c_void, net::NetError> = net::udp_socket;
    let _: fn(*mut c_void) = net::close;
    let _: fn(*mut c_void, u32) = net::set_recv_timeout;
    let _: fn() -> bool = net::is_network_up;
    let _: fn() -> u32 = net::get_ip_address;
    let _: fn(&str) -> Result<u32, net::NetError> = net::dns_resolve;
}
