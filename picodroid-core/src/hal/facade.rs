// SPDX-License-Identifier: GPL-3.0-only
//! The call-site face of the HAL seam.
//!
//! Shared code calls `hal::display::update_window()` exactly as it did when
//! the HAL was a module of free functions in the binary crate — that is the
//! point. Roughly 90 call sites across the framework keep their spelling;
//! only the crate they resolve in changes. Each wrapper forwards to an
//! `extern "Rust"` symbol that the active platform defines through the
//! `set_hal_*!` macros.
//!
//! **Symbol-name contract.** The `extern` declarations here and the
//! `#[no_mangle]` definitions in `macros.rs` are matched by the linker on
//! name alone, so they must agree on signature. Two things keep them honest:
//! this crate registers stub implementations through the very same macros
//! under `cfg(test)`, which puts declaration and definition in one crate
//! where `clashing_extern_declarations` fires; and every generated body goes
//! through `<T as Trait>::method`, so an implementation that drifts from the
//! contract fails to compile at its registration site.
//!
//! Calls are `unsafe` only in the FFI-declaration sense. There is exactly one
//! definition per symbol in any link, resolved at link time with no
//! indirection — a direct call, the same cost a cross-module call already had
//! under the no-LTO embedded profile.

pub mod display {
    extern "Rust" {
        fn __pd_hal_display_init();
        fn __pd_hal_display_set_window(x0: u16, y0: u16, x1: u16, y1: u16);
        fn __pd_hal_display_write_pixels(data: &[u8]);
        fn __pd_hal_display_set_backlight(on: bool);
        fn __pd_hal_display_sleep();
        fn __pd_hal_display_wake();
        fn __pd_hal_display_update_window();
        fn __pd_hal_display_is_window_open() -> bool;
    }

    pub fn init() {
        unsafe { __pd_hal_display_init() }
    }
    pub fn set_window(x0: u16, y0: u16, x1: u16, y1: u16) {
        unsafe { __pd_hal_display_set_window(x0, y0, x1, y1) }
    }
    pub fn write_pixels(data: &[u8]) {
        unsafe { __pd_hal_display_write_pixels(data) }
    }
    pub fn set_backlight(on: bool) {
        unsafe { __pd_hal_display_set_backlight(on) }
    }
    pub fn display_sleep() {
        unsafe { __pd_hal_display_sleep() }
    }
    pub fn display_wake() {
        unsafe { __pd_hal_display_wake() }
    }
    pub fn update_window() {
        unsafe { __pd_hal_display_update_window() }
    }
    pub fn is_window_open() -> bool {
        unsafe { __pd_hal_display_is_window_open() }
    }

    // Geometry is board data, not platform behaviour — re-exported here so
    // `hal::display::WIDTH` keeps resolving for code that moved in from the
    // binary crate.
    pub use crate::board_cfg::display::{
        BAND_HEIGHT, SCREEN_HEIGHT as HEIGHT, SCREEN_WIDTH as WIDTH, SCROLL_LIMIT,
    };
}

pub mod gpio {
    // Re-exported, not merely imported: call sites that moved in from the
    // binary crate spell these `hal::gpio::Pull` etc., because that is where
    // the per-family HAL defined them.
    pub use crate::hal::types::{EdgeTrigger, GpioEvent, Pull};

    extern "Rust" {
        fn __pd_hal_gpio_set_direction(pin: u8, direction: i32);
        fn __pd_hal_gpio_set_value(pin: u8, high: bool);
        fn __pd_hal_gpio_set_input(pin: u8, pull: Pull);
        fn __pd_hal_gpio_read(pin: u8) -> bool;
        fn __pd_hal_gpio_enable_edge_irq(pin: u8, edge: EdgeTrigger);
        fn __pd_hal_gpio_disable_edge_irq(pin: u8);
        fn __pd_hal_gpio_init_gpio_irq();
        fn __pd_hal_gpio_inject(pin: u8, rising: bool);
        fn __pd_hal_gpio_drain_gpio_event() -> Option<GpioEvent>;
        fn __pd_hal_gpio_has_pending_event() -> bool;
        fn __pd_hal_gpio_wait_for_button_event();
    }

    pub fn set_direction(pin: u8, direction: i32) {
        unsafe { __pd_hal_gpio_set_direction(pin, direction) }
    }
    pub fn set_value(pin: u8, high: bool) {
        unsafe { __pd_hal_gpio_set_value(pin, high) }
    }
    pub fn set_input(pin: u8, pull: Pull) {
        unsafe { __pd_hal_gpio_set_input(pin, pull) }
    }
    pub fn read(pin: u8) -> bool {
        unsafe { __pd_hal_gpio_read(pin) }
    }
    pub fn enable_edge_irq(pin: u8, edge: EdgeTrigger) {
        unsafe { __pd_hal_gpio_enable_edge_irq(pin, edge) }
    }
    pub fn disable_edge_irq(pin: u8) {
        unsafe { __pd_hal_gpio_disable_edge_irq(pin) }
    }
    pub fn init_gpio_irq() {
        unsafe { __pd_hal_gpio_init_gpio_irq() }
    }
    pub fn inject(pin: u8, rising: bool) {
        unsafe { __pd_hal_gpio_inject(pin, rising) }
    }
    pub fn drain_gpio_event() -> Option<GpioEvent> {
        unsafe { __pd_hal_gpio_drain_gpio_event() }
    }
    pub fn has_pending_event() -> bool {
        unsafe { __pd_hal_gpio_has_pending_event() }
    }
    pub fn wait_for_button_event() {
        unsafe { __pd_hal_gpio_wait_for_button_event() }
    }
}

pub mod system_clock {
    extern "Rust" {
        fn __pd_hal_clock_sleep(ms: u32);
        fn __pd_hal_clock_elapsed_realtime_nanos() -> i64;
    }

    pub fn sleep(ms: u32) {
        unsafe { __pd_hal_clock_sleep(ms) }
    }
    pub fn elapsed_realtime_nanos() -> i64 {
        unsafe { __pd_hal_clock_elapsed_realtime_nanos() }
    }
}

pub mod touch {
    extern "Rust" {
        fn __pd_hal_touch_init();
        fn __pd_hal_touch_read_point() -> Option<(u16, u16)>;
        fn __pd_hal_touch_read_raw_unfiltered() -> (u16, u16);
        fn __pd_hal_touch_set_calibration(x_min: u16, x_max: u16, y_min: u16, y_max: u16);
        fn __pd_hal_touch_inject_override(x: u16, y: u16);
        fn __pd_hal_touch_release_override();
        fn __pd_hal_touch_clear_override();
    }

    pub fn init() {
        unsafe { __pd_hal_touch_init() }
    }
    pub fn read_point() -> Option<(u16, u16)> {
        unsafe { __pd_hal_touch_read_point() }
    }
    pub fn read_raw_unfiltered() -> (u16, u16) {
        unsafe { __pd_hal_touch_read_raw_unfiltered() }
    }
    pub fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16) {
        unsafe { __pd_hal_touch_set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max) }
    }
    pub fn inject_override(x: u16, y: u16) {
        unsafe { __pd_hal_touch_inject_override(x, y) }
    }
    pub fn release_override() {
        unsafe { __pd_hal_touch_release_override() }
    }
    pub fn clear_override() {
        unsafe { __pd_hal_touch_clear_override() }
    }
}

pub mod i2c {
    use pico_jvm::array_heap::ArrayHeap;

    extern "Rust" {
        fn __pd_hal_i2c_init(i2c_id: u8);
        fn __pd_hal_i2c_set_speed(i2c_id: u8, hz: u32);
        fn __pd_hal_i2c_write_slice(i2c_id: u8, address: u8, data: &[u8]) -> i32;
        fn __pd_hal_i2c_read_slice(i2c_id: u8, address: u8, buf: &mut [u8]) -> i32;
        fn __pd_hal_i2c_write(
            i2c_id: u8,
            address: u32,
            data_idx: u16,
            len: usize,
            arrays: &ArrayHeap,
        ) -> i32;
        fn __pd_hal_i2c_read(
            i2c_id: u8,
            address: u32,
            buf_idx: u16,
            len: usize,
            arrays: &mut ArrayHeap,
        ) -> i32;
    }

    pub fn init(i2c_id: u8) {
        unsafe { __pd_hal_i2c_init(i2c_id) }
    }
    pub fn set_speed(i2c_id: u8, hz: u32) {
        unsafe { __pd_hal_i2c_set_speed(i2c_id, hz) }
    }
    pub fn write_slice(i2c_id: u8, address: u8, data: &[u8]) -> i32 {
        unsafe { __pd_hal_i2c_write_slice(i2c_id, address, data) }
    }
    pub fn read_slice(i2c_id: u8, address: u8, buf: &mut [u8]) -> i32 {
        unsafe { __pd_hal_i2c_read_slice(i2c_id, address, buf) }
    }
    pub fn write(i2c_id: u8, address: u32, data_idx: u16, len: usize, arrays: &ArrayHeap) -> i32 {
        unsafe { __pd_hal_i2c_write(i2c_id, address, data_idx, len, arrays) }
    }
    pub fn read(i2c_id: u8, address: u32, buf_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
        unsafe { __pd_hal_i2c_read(i2c_id, address, buf_idx, len, arrays) }
    }
}

pub mod adc {
    extern "Rust" {
        fn __pd_hal_adc_init(pin: u8);
        fn __pd_hal_adc_read(pin: u8) -> f64;
    }

    pub fn init(pin: u8) {
        unsafe { __pd_hal_adc_init(pin) }
    }
    pub fn read(pin: u8) -> f64 {
        unsafe { __pd_hal_adc_read(pin) }
    }
}

pub mod pwm {
    extern "Rust" {
        fn __pd_hal_pwm_init(pin: u8);
        fn __pd_hal_pwm_apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool);
    }

    pub fn init(pin: u8) {
        unsafe { __pd_hal_pwm_init(pin) }
    }
    pub fn apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool) {
        unsafe { __pd_hal_pwm_apply(pin, freq_hz, duty_cycle, enabled) }
    }
}

pub mod spi {
    use pico_jvm::array_heap::ArrayHeap;

    extern "Rust" {
        fn __pd_hal_spi_init(spi_id: u8);
        fn __pd_hal_spi_reconfigure(spi_id: u8, freq_hz: u32, mode: u32);
        fn __pd_hal_spi_write_raw(spi_id: u8, data: &[u8]);
        fn __pd_hal_spi_transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8]);
        fn __pd_hal_spi_transfer(
            spi_id: u8,
            tx_idx: u16,
            rx_idx: u16,
            len: usize,
            arrays: &mut ArrayHeap,
        ) -> i32;
        fn __pd_hal_spi_write(spi_id: u8, data_idx: u16, len: usize, arrays: &ArrayHeap) -> i32;
    }

    pub fn init(spi_id: u8) {
        unsafe { __pd_hal_spi_init(spi_id) }
    }
    pub fn reconfigure(spi_id: u8, freq_hz: u32, mode: u32) {
        unsafe { __pd_hal_spi_reconfigure(spi_id, freq_hz, mode) }
    }
    pub fn write_raw(spi_id: u8, data: &[u8]) {
        unsafe { __pd_hal_spi_write_raw(spi_id, data) }
    }
    pub fn transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8]) {
        unsafe { __pd_hal_spi_transfer_raw(spi_id, tx, rx) }
    }
    pub fn transfer(
        spi_id: u8,
        tx_idx: u16,
        rx_idx: u16,
        len: usize,
        arrays: &mut ArrayHeap,
    ) -> i32 {
        unsafe { __pd_hal_spi_transfer(spi_id, tx_idx, rx_idx, len, arrays) }
    }
    pub fn write(spi_id: u8, data_idx: u16, len: usize, arrays: &ArrayHeap) -> i32 {
        unsafe { __pd_hal_spi_write(spi_id, data_idx, len, arrays) }
    }
}

pub mod uart {
    extern "Rust" {
        fn __pd_hal_uart_init(uart_id: u8);
        fn __pd_hal_uart_write_byte(uart_id: u8, byte: u8);
        fn __pd_hal_uart_read_byte(uart_id: u8) -> i32;
        fn __pd_hal_uart_reconfigure(
            uart_id: u8,
            baudrate: i32,
            data_size: i32,
            parity: i32,
            stop_bits: i32,
            hw_flow: i32,
        );
    }

    pub fn init(uart_id: u8) {
        unsafe { __pd_hal_uart_init(uart_id) }
    }
    pub fn write_byte(uart_id: u8, byte: u8) {
        unsafe { __pd_hal_uart_write_byte(uart_id, byte) }
    }
    pub fn read_byte(uart_id: u8) -> i32 {
        unsafe { __pd_hal_uart_read_byte(uart_id) }
    }
    pub fn reconfigure(
        uart_id: u8,
        baudrate: i32,
        data_size: i32,
        parity: i32,
        stop_bits: i32,
        hw_flow: i32,
    ) {
        unsafe {
            __pd_hal_uart_reconfigure(uart_id, baudrate, data_size, parity, stop_bits, hw_flow)
        }
    }
}

#[cfg(has_network)]
// Socket handles are opaque: `tcp_socket`/`udp_socket` mint them, callers
// pass them straight back, and only the platform's network stack ever
// dereferences one. Shared code cannot construct a socket pointer, so these
// stay safe fns — the same shape the family HAL already had before the
// extraction. Marking them `unsafe` would push `unsafe` blocks onto every
// call site in system/picodroid/net without adding a checkable invariant.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub mod net {
    use core::ffi::c_void;

    // Re-exported for the same reason as the gpio types: shared net code
    // spells this `hal::net::NetError`.
    pub use crate::hal::types::NetError;

    extern "Rust" {
        fn __pd_hal_net_tcp_socket() -> Result<*mut c_void, NetError>;
        fn __pd_hal_net_tcp_connect(
            sock: *mut c_void,
            addr: u32,
            port: u16,
        ) -> Result<(), NetError>;
        fn __pd_hal_net_tcp_send(sock: *mut c_void, data: &[u8]) -> Result<usize, NetError>;
        fn __pd_hal_net_tcp_recv(sock: *mut c_void, buf: &mut [u8]) -> Result<usize, NetError>;
        fn __pd_hal_net_tcp_listen(sock: *mut c_void, port: u16) -> Result<(), NetError>;
        fn __pd_hal_net_tcp_accept(sock: *mut c_void) -> Result<*mut c_void, NetError>;
        fn __pd_hal_net_udp_socket(local_port: u16) -> Result<*mut c_void, NetError>;
        fn __pd_hal_net_udp_sendto(
            sock: *mut c_void,
            buf: &[u8],
            addr: u32,
            port: u16,
        ) -> Result<usize, NetError>;
        fn __pd_hal_net_udp_recvfrom(
            sock: *mut c_void,
            buf: &mut [u8],
        ) -> Result<(usize, u32, u16), NetError>;
        fn __pd_hal_net_close(sock: *mut c_void);
        fn __pd_hal_net_set_recv_timeout(sock: *mut c_void, ms: u32);
        fn __pd_hal_net_is_network_up() -> bool;
        fn __pd_hal_net_get_ip_address() -> u32;
        fn __pd_hal_net_dns_resolve(hostname: &str) -> Result<u32, NetError>;
    }

    pub fn tcp_socket() -> Result<*mut c_void, NetError> {
        unsafe { __pd_hal_net_tcp_socket() }
    }
    pub fn tcp_connect(sock: *mut c_void, addr: u32, port: u16) -> Result<(), NetError> {
        unsafe { __pd_hal_net_tcp_connect(sock, addr, port) }
    }
    pub fn tcp_send(sock: *mut c_void, data: &[u8]) -> Result<usize, NetError> {
        unsafe { __pd_hal_net_tcp_send(sock, data) }
    }
    pub fn tcp_recv(sock: *mut c_void, buf: &mut [u8]) -> Result<usize, NetError> {
        unsafe { __pd_hal_net_tcp_recv(sock, buf) }
    }
    pub fn tcp_listen(sock: *mut c_void, port: u16) -> Result<(), NetError> {
        unsafe { __pd_hal_net_tcp_listen(sock, port) }
    }
    pub fn tcp_accept(sock: *mut c_void) -> Result<*mut c_void, NetError> {
        unsafe { __pd_hal_net_tcp_accept(sock) }
    }
    pub fn udp_socket(local_port: u16) -> Result<*mut c_void, NetError> {
        unsafe { __pd_hal_net_udp_socket(local_port) }
    }
    pub fn udp_sendto(
        sock: *mut c_void,
        buf: &[u8],
        addr: u32,
        port: u16,
    ) -> Result<usize, NetError> {
        unsafe { __pd_hal_net_udp_sendto(sock, buf, addr, port) }
    }
    pub fn udp_recvfrom(sock: *mut c_void, buf: &mut [u8]) -> Result<(usize, u32, u16), NetError> {
        unsafe { __pd_hal_net_udp_recvfrom(sock, buf) }
    }
    pub fn close(sock: *mut c_void) {
        unsafe { __pd_hal_net_close(sock) }
    }
    pub fn set_recv_timeout(sock: *mut c_void, ms: u32) {
        unsafe { __pd_hal_net_set_recv_timeout(sock, ms) }
    }
    pub fn is_network_up() -> bool {
        unsafe { __pd_hal_net_is_network_up() }
    }
    pub fn get_ip_address() -> u32 {
        unsafe { __pd_hal_net_get_ip_address() }
    }
    pub fn dns_resolve(hostname: &str) -> Result<u32, NetError> {
        unsafe { __pd_hal_net_dns_resolve(hostname) }
    }
}

/// Filesystem — see [`crate::hal::HalFs`] for why this is path-in / value-out.
pub mod fs {
    use alloc::vec::Vec;

    extern "Rust" {
        fn __pd_hal_fs_exists(path: &str) -> bool;
        fn __pd_hal_fs_is_file(path: &str) -> bool;
        fn __pd_hal_fs_is_dir(path: &str) -> bool;
        fn __pd_hal_fs_length(path: &str) -> i64;
        fn __pd_hal_fs_delete(path: &str) -> bool;
        fn __pd_hal_fs_mkdir(path: &str) -> bool;
        fn __pd_hal_fs_rename(from: &str, to: &str) -> bool;
        fn __pd_hal_fs_truncate(path: &str);
        fn __pd_hal_fs_read_at(path: &str, pos: u64, out: &mut Vec<u8>, len: usize) -> i32;
        fn __pd_hal_fs_write_at(path: &str, pos: u64, data: &[u8]) -> i32;
    }

    pub fn exists(path: &str) -> bool {
        unsafe { __pd_hal_fs_exists(path) }
    }
    pub fn is_file(path: &str) -> bool {
        unsafe { __pd_hal_fs_is_file(path) }
    }
    pub fn is_dir(path: &str) -> bool {
        unsafe { __pd_hal_fs_is_dir(path) }
    }
    pub fn length(path: &str) -> i64 {
        unsafe { __pd_hal_fs_length(path) }
    }
    pub fn delete(path: &str) -> bool {
        unsafe { __pd_hal_fs_delete(path) }
    }
    pub fn mkdir(path: &str) -> bool {
        unsafe { __pd_hal_fs_mkdir(path) }
    }
    pub fn rename(from: &str, to: &str) -> bool {
        unsafe { __pd_hal_fs_rename(from, to) }
    }
    pub fn truncate(path: &str) {
        unsafe { __pd_hal_fs_truncate(path) }
    }
    pub fn read_at(path: &str, pos: u64, out: &mut Vec<u8>, len: usize) -> i32 {
        unsafe { __pd_hal_fs_read_at(path, pos, out, len) }
    }
    pub fn write_at(path: &str, pos: u64, data: &[u8]) -> i32 {
        unsafe { __pd_hal_fs_write_at(path, pos, data) }
    }
}
