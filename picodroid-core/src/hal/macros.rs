// SPDX-License-Identifier: GPL-3.0-only
//! HAL registration macros.
//!
//! A platform crate implements the [`crate::hal`] traits for its own types
//! and calls these macros once, which emits the `#[no_mangle]` shims the
//! facade's `extern "Rust"` declarations bind to at link time.
//!
//! There is one macro per subsystem so a bring-up can adopt the seam
//! incrementally (display first, then input, then the rest), plus
//! [`set_hal!`] for the common case of registering everything at once.
//!
//! Every generated body dispatches through `<$t as Trait>::method`, so the
//! trait — not the macro — decides the signature. An implementation that
//! drifts from HAL CONTRACT v2 fails to compile right here at the
//! registration site, which is what lets `hal/contract.rs`'s hand-written
//! `_assert_*` bindings retire for trait-covered modules.
//!
//! Exactly one registration may be linked into a binary. Registering twice
//! is a duplicate-symbol link error, which is the intended outcome: sim and
//! device arms must be `cfg`-exclusive.

/// Register the display HAL.
#[macro_export]
macro_rules! set_hal_display {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_display_init() {
                <$t as $crate::hal::HalDisplay>::init()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_display_set_window(x0: u16, y0: u16, x1: u16, y1: u16) {
                <$t as $crate::hal::HalDisplay>::set_window(x0, y0, x1, y1)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_display_write_pixels(data: &[u8]) {
                <$t as $crate::hal::HalDisplay>::write_pixels(data)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_display_set_backlight(on: bool) {
                <$t as $crate::hal::HalDisplay>::set_backlight(on)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_display_sleep() {
                <$t as $crate::hal::HalDisplay>::display_sleep()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_display_wake() {
                <$t as $crate::hal::HalDisplay>::display_wake()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_display_update_window() {
                <$t as $crate::hal::HalDisplay>::update_window()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_display_is_window_open() -> bool {
                <$t as $crate::hal::HalDisplay>::is_window_open()
            }
        };
    };
}

/// Register the GPIO HAL.
#[macro_export]
macro_rules! set_hal_gpio {
    ($t:ty) => {
        const _: () = {
            use $crate::hal::types::{EdgeTrigger, GpioEvent, Pull};

            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_set_direction(pin: u8, direction: i32) {
                <$t as $crate::hal::HalGpio>::set_direction(pin, direction)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_set_value(pin: u8, high: bool) {
                <$t as $crate::hal::HalGpio>::set_value(pin, high)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_set_input(pin: u8, pull: Pull) {
                <$t as $crate::hal::HalGpio>::set_input(pin, pull)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_read(pin: u8) -> bool {
                <$t as $crate::hal::HalGpio>::read(pin)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_enable_edge_irq(pin: u8, edge: EdgeTrigger) {
                <$t as $crate::hal::HalGpio>::enable_edge_irq(pin, edge)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_disable_edge_irq(pin: u8) {
                <$t as $crate::hal::HalGpio>::disable_edge_irq(pin)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_init_gpio_irq() {
                <$t as $crate::hal::HalGpio>::init_gpio_irq()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_inject(pin: u8, rising: bool) {
                <$t as $crate::hal::HalGpio>::inject(pin, rising)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_drain_gpio_event() -> Option<GpioEvent> {
                <$t as $crate::hal::HalGpio>::drain_gpio_event()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_has_pending_event() -> bool {
                <$t as $crate::hal::HalGpio>::has_pending_event()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_gpio_wait_for_button_event() {
                <$t as $crate::hal::HalGpio>::wait_for_button_event()
            }
        };
    };
}

/// Register the system clock HAL.
#[macro_export]
macro_rules! set_hal_clock {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_clock_sleep(ms: u32) {
                <$t as $crate::hal::HalClock>::sleep(ms)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_clock_elapsed_realtime_nanos() -> i64 {
                <$t as $crate::hal::HalClock>::elapsed_realtime_nanos()
            }
        };
    };
}

/// Register the touch HAL.
#[macro_export]
macro_rules! set_hal_touch {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_touch_init() {
                <$t as $crate::hal::HalTouch>::init()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_touch_read_point() -> Option<(u16, u16)> {
                <$t as $crate::hal::HalTouch>::read_point()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_touch_read_raw_unfiltered() -> (u16, u16) {
                <$t as $crate::hal::HalTouch>::read_raw_unfiltered()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_touch_set_calibration(
                x_min: u16,
                x_max: u16,
                y_min: u16,
                y_max: u16,
            ) {
                <$t as $crate::hal::HalTouch>::set_calibration(x_min, x_max, y_min, y_max)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_touch_inject_override(x: u16, y: u16) {
                <$t as $crate::hal::HalTouch>::inject_override(x, y)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_touch_release_override() {
                <$t as $crate::hal::HalTouch>::release_override()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_touch_clear_override() {
                <$t as $crate::hal::HalTouch>::clear_override()
            }
        };
    };
}

/// Register the I2C HAL.
#[macro_export]
macro_rules! set_hal_i2c {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_i2c_init(i2c_id: u8) {
                <$t as $crate::hal::HalI2c>::init(i2c_id)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_i2c_set_speed(i2c_id: u8, hz: u32) {
                <$t as $crate::hal::HalI2c>::set_speed(i2c_id, hz)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_i2c_write_slice(i2c_id: u8, address: u8, data: &[u8]) -> i32 {
                <$t as $crate::hal::HalI2c>::write_slice(i2c_id, address, data)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_i2c_read_slice(
                i2c_id: u8,
                address: u8,
                buf: &mut [u8],
            ) -> i32 {
                <$t as $crate::hal::HalI2c>::read_slice(i2c_id, address, buf)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_i2c_write(
                i2c_id: u8,
                address: u32,
                data_idx: u16,
                len: usize,
                arrays: &::pico_jvm::array_heap::ArrayHeap,
            ) -> i32 {
                <$t as $crate::hal::HalI2c>::write(i2c_id, address, data_idx, len, arrays)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_i2c_read(
                i2c_id: u8,
                address: u32,
                buf_idx: u16,
                len: usize,
                arrays: &mut ::pico_jvm::array_heap::ArrayHeap,
            ) -> i32 {
                <$t as $crate::hal::HalI2c>::read(i2c_id, address, buf_idx, len, arrays)
            }
        };
    };
}

/// Register the ADC HAL.
#[macro_export]
macro_rules! set_hal_adc {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_adc_init(pin: u8) {
                <$t as $crate::hal::HalAdc>::init(pin)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_adc_read(pin: u8) -> f64 {
                <$t as $crate::hal::HalAdc>::read(pin)
            }
        };
    };
}

/// Register the PWM HAL.
#[macro_export]
macro_rules! set_hal_pwm {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_pwm_init(pin: u8) {
                <$t as $crate::hal::HalPwm>::init(pin)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_pwm_apply(
                pin: u8,
                freq_hz: f64,
                duty_cycle: f64,
                enabled: bool,
            ) {
                <$t as $crate::hal::HalPwm>::apply(pin, freq_hz, duty_cycle, enabled)
            }
        };
    };
}

/// Register the SPI HAL.
#[macro_export]
macro_rules! set_hal_spi {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_spi_init(spi_id: u8) {
                <$t as $crate::hal::HalSpi>::init(spi_id)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_spi_reconfigure(spi_id: u8, freq_hz: u32, mode: u32) {
                <$t as $crate::hal::HalSpi>::reconfigure(spi_id, freq_hz, mode)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_spi_write_raw(spi_id: u8, data: &[u8]) {
                <$t as $crate::hal::HalSpi>::write_raw(spi_id, data)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_spi_transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8]) {
                <$t as $crate::hal::HalSpi>::transfer_raw(spi_id, tx, rx)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_spi_transfer(
                spi_id: u8,
                tx_idx: u16,
                rx_idx: u16,
                len: usize,
                arrays: &mut ::pico_jvm::array_heap::ArrayHeap,
            ) -> i32 {
                <$t as $crate::hal::HalSpi>::transfer(spi_id, tx_idx, rx_idx, len, arrays)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_spi_write(
                spi_id: u8,
                data_idx: u16,
                len: usize,
                arrays: &::pico_jvm::array_heap::ArrayHeap,
            ) -> i32 {
                <$t as $crate::hal::HalSpi>::write(spi_id, data_idx, len, arrays)
            }
        };
    };
}

/// Register the UART HAL.
#[macro_export]
macro_rules! set_hal_uart {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_uart_init(uart_id: u8) {
                <$t as $crate::hal::HalUart>::init(uart_id)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_uart_write_byte(uart_id: u8, byte: u8) {
                <$t as $crate::hal::HalUart>::write_byte(uart_id, byte)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_uart_read_byte(uart_id: u8) -> i32 {
                <$t as $crate::hal::HalUart>::read_byte(uart_id)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_uart_reconfigure(
                uart_id: u8,
                baudrate: i32,
                data_size: i32,
                parity: i32,
                stop_bits: i32,
                hw_flow: i32,
            ) {
                <$t as $crate::hal::HalUart>::reconfigure(
                    uart_id, baudrate, data_size, parity, stop_bits, hw_flow,
                )
            }
        };
    };
}

/// Register the network HAL. Only meaningful under `cfg(has_network)`; the
/// facade module it feeds is gated on the same cfg.
#[macro_export]
macro_rules! set_hal_net {
    ($t:ty) => {
        const _: () = {
            use core::ffi::c_void;
            use $crate::hal::types::NetError;

            #[no_mangle]
            extern "Rust" fn __pd_hal_net_tcp_socket() -> Result<*mut c_void, NetError> {
                <$t as $crate::hal::HalNet>::tcp_socket()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_tcp_connect(
                sock: *mut c_void,
                addr: u32,
                port: u16,
            ) -> Result<(), NetError> {
                <$t as $crate::hal::HalNet>::tcp_connect(sock, addr, port)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_tcp_send(
                sock: *mut c_void,
                data: &[u8],
            ) -> Result<usize, NetError> {
                <$t as $crate::hal::HalNet>::tcp_send(sock, data)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_tcp_recv(
                sock: *mut c_void,
                buf: &mut [u8],
            ) -> Result<usize, NetError> {
                <$t as $crate::hal::HalNet>::tcp_recv(sock, buf)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_tcp_listen(
                sock: *mut c_void,
                port: u16,
            ) -> Result<(), NetError> {
                <$t as $crate::hal::HalNet>::tcp_listen(sock, port)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_tcp_accept(
                sock: *mut c_void,
            ) -> Result<*mut c_void, NetError> {
                <$t as $crate::hal::HalNet>::tcp_accept(sock)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_udp_socket(
                local_port: u16,
            ) -> Result<*mut c_void, NetError> {
                <$t as $crate::hal::HalNet>::udp_socket(local_port)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_udp_sendto(
                sock: *mut c_void,
                buf: &[u8],
                addr: u32,
                port: u16,
            ) -> Result<usize, NetError> {
                <$t as $crate::hal::HalNet>::udp_sendto(sock, buf, addr, port)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_udp_recvfrom(
                sock: *mut c_void,
                buf: &mut [u8],
            ) -> Result<(usize, u32, u16), NetError> {
                <$t as $crate::hal::HalNet>::udp_recvfrom(sock, buf)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_close(sock: *mut c_void) {
                <$t as $crate::hal::HalNet>::close(sock)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_set_recv_timeout(sock: *mut c_void, ms: u32) {
                <$t as $crate::hal::HalNet>::set_recv_timeout(sock, ms)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_is_network_up() -> bool {
                <$t as $crate::hal::HalNet>::is_network_up()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_get_ip_address() -> u32 {
                <$t as $crate::hal::HalNet>::get_ip_address()
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_net_dns_resolve(hostname: &str) -> Result<u32, NetError> {
                <$t as $crate::hal::HalNet>::dns_resolve(hostname)
            }
        };
    };
}

/// Register every always-required HAL subsystem in one call.
///
/// `net` is deliberately absent — it is `cfg(has_network)`-gated, so
/// platforms call [`set_hal_net!`] separately under that cfg.
///
/// ```ignore
/// picodroid_core::set_hal! {
///     display = RpDisplay,
///     gpio    = RpGpio,
///     clock   = RpClock,
///     touch   = RpTouch,
///     i2c     = RpI2c,
///     adc     = RpAdc,
///     pwm     = RpPwm,
///     spi     = RpSpi,
///     uart    = RpUart,
/// }
/// ```
#[macro_export]
macro_rules! set_hal {
    (
        display = $display:ty,
        gpio    = $gpio:ty,
        clock   = $clock:ty,
        touch   = $touch:ty,
        i2c     = $i2c:ty,
        adc     = $adc:ty,
        pwm     = $pwm:ty,
        spi     = $spi:ty,
        uart    = $uart:ty $(,)?
    ) => {
        $crate::set_hal_display!($display);
        $crate::set_hal_gpio!($gpio);
        $crate::set_hal_clock!($clock);
        $crate::set_hal_touch!($touch);
        $crate::set_hal_i2c!($i2c);
        $crate::set_hal_adc!($adc);
        $crate::set_hal_pwm!($pwm);
        $crate::set_hal_spi!($spi);
        $crate::set_hal_uart!($uart);
    };
}

/// Bind [`HalFs`](crate::hal::HalFs) to `$t`.
///
/// Separate from [`set_hal`] rather than folded into it, because storage is
/// conditional the way networking is: a platform that has no filesystem in a
/// given build simply does not invoke this. The RP crate's `mod fs` is
/// `cfg(not(test))`, so its host-test build is exactly that case.
#[macro_export]
macro_rules! set_hal_fs {
    ($t:ty) => {
        const _: () = {
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_exists(path: &str) -> bool {
                <$t as $crate::hal::HalFs>::exists(path)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_is_file(path: &str) -> bool {
                <$t as $crate::hal::HalFs>::is_file(path)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_is_dir(path: &str) -> bool {
                <$t as $crate::hal::HalFs>::is_dir(path)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_length(path: &str) -> i64 {
                <$t as $crate::hal::HalFs>::length(path)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_delete(path: &str) -> bool {
                <$t as $crate::hal::HalFs>::delete(path)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_mkdir(path: &str) -> bool {
                <$t as $crate::hal::HalFs>::mkdir(path)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_rename(from: &str, to: &str) -> bool {
                <$t as $crate::hal::HalFs>::rename(from, to)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_truncate(path: &str) {
                <$t as $crate::hal::HalFs>::truncate(path)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_read_at(
                path: &str,
                pos: u64,
                out: &mut ::alloc::vec::Vec<u8>,
                len: usize,
            ) -> i32 {
                <$t as $crate::hal::HalFs>::read_at(path, pos, out, len)
            }
            #[no_mangle]
            extern "Rust" fn __pd_hal_fs_write_at(path: &str, pos: u64, data: &[u8]) -> i32 {
                <$t as $crate::hal::HalFs>::write_at(path, pos, data)
            }
        };
    };
}
