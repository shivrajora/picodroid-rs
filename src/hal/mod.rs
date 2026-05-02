// SPDX-License-Identifier: GPL-3.0-only
//! Hardware Abstraction Layer — centralises all chip-specific code.
//!
//! A single `#[cfg]` dispatch selects the chip family module.  Each family
//! (rp, esp, …) provides the same set of public free functions so that the
//! rest of the crate can call `hal::uart::init()` etc. without knowing which
//! chip is underneath.
//!
//! # HAL CONTRACT v1
//!
//! Every chip family module (e.g. `src/hal/rp/`, `src/hal/sim/`, future
//! `src/hal/esp/`) must expose the public symbols listed below. The
//! [`contract`] sub-module turns this list into compile-time assertions —
//! drift in any signature breaks `cargo test --no-run` and `cargo clippy`.
//!
//! Symbols are grouped by the cfg under which they are required.
//!
//! ## Always required (sim + every hardware family)
//!
//! `adc`:
//! - `pub fn init(pin: u8)`
//! - `pub fn read(pin: u8) -> f64`
//!
//! `display`:
//! - `pub const WIDTH: u16`, `HEIGHT: u16`, `BAND_HEIGHT: usize`, `SCROLL_LIMIT: u8`
//! - `pub fn init()`
//! - `pub fn set_window(x0: u16, y0: u16, x1: u16, y1: u16)`
//! - `pub fn write_pixels(data: &[u8])`
//! - `pub fn set_backlight(on: bool)`
//! - `pub fn display_sleep()`, `display_wake()`
//! - `pub fn update_window()`, `is_window_open() -> bool`
//!
//! `gpio`:
//! - `pub enum Pull`, `pub enum EdgeTrigger`, `pub struct GpioEvent`
//! - `pub fn set_direction(pin: u8, direction: i32)`
//! - `pub fn set_value(pin: u8, high: bool)`
//! - `pub fn set_input(pin: u8, pull: Pull)`
//! - `pub fn read(pin: u8) -> bool`
//! - `pub fn enable_edge_irq(pin: u8, edge: EdgeTrigger)`
//! - `pub fn disable_edge_irq(pin: u8)`
//! - `pub fn init_gpio_irq()`
//! - `pub fn drain_gpio_event() -> Option<GpioEvent>`
//! - `pub fn has_pending_event() -> bool`
//! - `pub fn wait_for_button_event()`
//!
//! `i2c`:
//! - `pub fn init(i2c_id: u8)`, `pub fn set_speed(i2c_id: u8, hz: u32)`
//! - `pub fn write_slice(i2c_id: u8, address: u8, data: &[u8]) -> i32`
//! - `pub fn read_slice(i2c_id: u8, address: u8, buf: &mut [u8]) -> i32`
//!
//! `pwm`:
//! - `pub fn init(pin: u8)`
//! - `pub fn apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool)`
//!
//! `spi`:
//! - `pub fn init(spi_id: u8)`, `pub fn reconfigure(spi_id: u8, freq_hz: u32, mode: u32)`
//! - `pub fn write_raw(spi_id: u8, data: &[u8])`
//! - `pub fn transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8])`
//!
//! `system_clock`:
//! - `pub fn sleep(ms: u32)`
//! - `pub fn elapsed_realtime_nanos() -> i64`
//!
//! `touch`:
//! - `pub fn init()`
//! - `pub fn read_point() -> Option<(u16, u16)>`
//! - `pub fn read_raw_unfiltered() -> (u16, u16)`
//! - `pub fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16)`
//!
//! `uart`:
//! - `pub fn init(uart_id: u8)`
//! - `pub fn write_byte(uart_id: u8, byte: u8)`
//! - `pub fn read_byte(uart_id: u8) -> i32`
//!
//! ## Required only on hardware (gated by `not(any(test, feature = "sim"))`)
//!
//! `boot`:
//! - `pub fn clock_init()`
//! - `pub fn start_tasks(boot_apk: &'static [u8]) -> !`
//!
//! `flash`:
//! - `pub const PAPK_MAX_DATA_SIZE: usize`
//! - `pub unsafe fn read_flash_papk() -> Option<&'static [u8]>`
//!
//! `pdb_usb`:
//! - `pub fn init()`, `drain_tx()`
//! - `pub fn queue_read_byte() -> u8`
//! - `pub fn queue_read_byte_timeout() -> Option<u8>`
//! - `pub fn queue_read_u32_le() -> u32`
//! - `pub fn write_bytes(data: &[u8])`
//!
//! Chip-within-family symbols (e.g. `pdb_usb::queue_read_byte_busywait`,
//! gated on `chip-rp2350`) are NOT part of the family contract — they are
//! conditionally compiled at the family-internal level and visible only at
//! their gated call sites.
//!
//! ## Required only when `cfg(has_network)`
//!
//! `net`:
//! - `pub struct NetError(pub i32)`
//! - `pub fn tcp_socket() -> Result<*mut c_void, NetError>`
//! - `pub fn tcp_connect(sock, addr, port)`, `tcp_send`, `tcp_recv`,
//!   `tcp_listen`, `tcp_accept`
//! - `pub fn udp_socket(local_port: u16)`, `udp_sendto`, `udp_recvfrom`
//! - `pub fn close(sock)`, `set_recv_timeout(sock, ms)`
//! - `pub fn is_network_up() -> bool`, `get_ip_address() -> u32`
//! - `pub fn dns_resolve(hostname: &str) -> Result<u32, NetError>`
//!
//! ## Internal-only (used by family-internal driver wiring; not part of the
//! cross-crate contract — name and shape are family-private)
//!
//! - `delay`, `input_pin`, `output_pin`, `spi_bus` — concrete types that
//!   implement `embedded_hal` traits, consumed by `src/hal/<family>/`
//!   internally to wire up `src/drivers/` generic drivers.

// In sim mode OR test mode, use the simulator stubs.
// (Tests run on the host where HAL crates like rp-pico are unavailable.)
#[cfg(any(feature = "sim", test))]
#[path = "sim/mod.rs"]
mod chip;

#[cfg(all(not(any(feature = "sim", test)), feature = "family-rp"))]
#[path = "rp/mod.rs"]
mod chip;

#[cfg(all(not(any(feature = "sim", test)), feature = "family-esp"))]
#[path = "esp/mod.rs"]
mod chip;

// Peripheral drivers
#[allow(unused_imports)]
pub use chip::adc;
#[allow(unused_imports)]
pub use chip::delay;
#[allow(unused_imports)]
pub use chip::display;
#[allow(unused_imports)]
pub use chip::gpio;
#[allow(unused_imports)]
pub use chip::i2c;
#[allow(unused_imports)]
pub use chip::input_pin;
#[allow(unused_imports)]
pub use chip::output_pin;
#[allow(unused_imports)]
pub use chip::pwm;
#[allow(unused_imports)]
pub use chip::spi;
#[allow(unused_imports)]
pub use chip::spi_bus;
#[allow(unused_imports)]
pub use chip::system_clock;
#[allow(unused_imports)]
pub use chip::touch;
#[allow(unused_imports)]
pub use chip::uart;

// Boot & flash (only meaningful on real hardware, but sim provides stubs
// for module completeness — suppress unused warnings in sim/test builds)
#[allow(unused_imports)]
pub use chip::boot;
#[allow(unused_imports)]
pub use chip::flash;
#[allow(unused_imports)]
pub use chip::pdb_usb;

#[cfg(has_network)]
#[allow(unused_imports)]
pub use chip::net;

// Compile-time HAL CONTRACT v1 enforcement. Never executed; type-checked only.
mod contract;
