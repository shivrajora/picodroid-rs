// SPDX-License-Identifier: GPL-3.0-only
//! HAL CONTRACT v2 — the trait form.
//!
//! v1 (`platforms/rp/src/hal/mod.rs`) specified the contract as a doc-block
//! listing free functions, enforced by dead `_assert_*` bindings in
//! `hal/contract.rs`. That works inside one crate but a *module* cannot cross
//! a crate boundary, which is exactly why the framework stayed trapped in the
//! binary crate (commit `1b3602b`). v2 says the same thing as traits, which
//! can.
//!
//! Signatures are lifted verbatim from v1's assertions, with two deliberate
//! changes:
//!
//! * The display *constants* left the contract — they are board data, not
//!   platform behaviour, and now come from [`crate::board_cfg::display`].
//! * `Pull` / `EdgeTrigger` / `GpioEvent` / `NetError` moved to
//!   [`crate::hal::types`], since shared code names them.
//!
//! Every method is an associated function: the platform registers a *type*,
//! and all HAL state lives in that family's statics, exactly as it did when
//! these were free functions.
//!
//! Implementations are wired up with the `set_hal_*!` macros, which generate
//! the `#[no_mangle]` shims the facade calls. Because each shim body goes
//! through `<T as Trait>::method`, an implementation whose signature drifts
//! from the contract fails to compile at the registration site — the macro
//! subsumes what `contract.rs` did.

use core::ffi::c_void;

use pico_jvm::array_heap::ArrayHeap;

use super::types::{EdgeTrigger, GpioEvent, NetError, Pull};

/// Framebuffer output. Geometry lives in [`crate::board_cfg::display`].
pub trait HalDisplay {
    fn init();
    fn set_window(x0: u16, y0: u16, x1: u16, y1: u16);
    fn write_pixels(data: &[u8]);
    fn set_backlight(on: bool);
    fn display_sleep();
    fn display_wake();
    /// Push the current framebuffer to the panel/window.
    fn update_window();
    /// False once the simulator window is closed; always true on hardware.
    fn is_window_open() -> bool;
}

/// Digital I/O and the button-edge interrupt queue.
pub trait HalGpio {
    fn set_direction(pin: u8, direction: i32);
    fn set_value(pin: u8, high: bool);
    fn set_input(pin: u8, pull: Pull);
    fn read(pin: u8) -> bool;
    fn enable_edge_irq(pin: u8, edge: EdgeTrigger);
    fn disable_edge_irq(pin: u8);
    fn init_gpio_irq();
    /// Inject a synthetic edge (PDB `CMD_INPUT`, simulator keyboard).
    fn inject(pin: u8, rising: bool);
    fn drain_gpio_event() -> Option<GpioEvent>;
    fn has_pending_event() -> bool;
    /// Block until an edge arrives — the idle path's sleep point.
    fn wait_for_button_event();
}

/// Monotonic time and coarse sleeping.
pub trait HalClock {
    fn sleep(ms: u32);
    fn elapsed_realtime_nanos() -> i64;
}

/// Resistive touch panel, including the scripted-input overrides PDB uses.
pub trait HalTouch {
    fn init();
    fn read_point() -> Option<(u16, u16)>;
    fn read_raw_unfiltered() -> (u16, u16);
    fn set_calibration(cal_x_min: u16, cal_x_max: u16, cal_y_min: u16, cal_y_max: u16);
    /// Scripted press/move (PDB `CMD_INPUT` tap/swipe).
    fn inject_override(x: u16, y: u16);
    /// Lift a scripted touch, producing a RELEASE edge.
    fn release_override();
    /// Resume sampling the real panel.
    fn clear_override();
}

pub trait HalI2c {
    /// Largest Java-array transfer the default [`HalI2c::write`] /
    /// [`HalI2c::read`] stage on the stack, in bytes; never more than
    /// [`crate::hal::array_io::STAGING_CAP`]. A family whose bus takes more
    /// overrides the two methods rather than this.
    const JAVA_XFER_MAX: usize = crate::hal::array_io::STAGING_CAP;

    fn init(i2c_id: u8);
    fn set_speed(i2c_id: u8, hz: u32);
    fn write_slice(i2c_id: u8, address: u8, data: &[u8]) -> i32;
    fn read_slice(i2c_id: u8, address: u8, buf: &mut [u8]) -> i32;

    /// Transfer straight out of / into a JVM byte array, addressed by heap
    /// index rather than a slice — the `picodroid.pio.I2cDevice` entry point.
    ///
    /// Defaulted over [`HalI2c::write_slice`] through
    /// [`crate::hal::array_io`]: the staging, the cap check and the copy are
    /// the same for every family, so a family writes the slice pair and
    /// nothing else. Returns `-1` when `len` exceeds `JAVA_XFER_MAX`, else
    /// what the bus said.
    fn write(i2c_id: u8, address: u32, data_idx: u16, len: usize, arrays: &ArrayHeap) -> i32 {
        crate::hal::array_io::i2c_write(Self::JAVA_XFER_MAX, arrays, data_idx, len, |data| {
            Self::write_slice(i2c_id, address as u8, data)
        })
    }

    /// The read half of [`HalI2c::write`]: `0` for `len == 0`, `-1` past the
    /// cap, else the bus result with the bytes it produced copied back.
    fn read(i2c_id: u8, address: u32, buf_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
        crate::hal::array_io::i2c_read(Self::JAVA_XFER_MAX, arrays, buf_idx, len, |buf| {
            Self::read_slice(i2c_id, address as u8, buf)
        })
    }
}

pub trait HalAdc {
    fn init(pin: u8);
    fn read(pin: u8) -> f64;
}

pub trait HalPwm {
    fn init(pin: u8);
    fn apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool);
}

pub trait HalSpi {
    /// Largest Java-array transfer the default [`HalSpi::transfer`] /
    /// [`HalSpi::write`] stage on the stack, in bytes; never more than
    /// [`crate::hal::array_io::STAGING_CAP`]. A family whose bus takes more
    /// overrides the two methods rather than this.
    const JAVA_XFER_MAX: usize = crate::hal::array_io::STAGING_CAP;

    fn init(spi_id: u8);
    fn reconfigure(spi_id: u8, freq_hz: u32, mode: u32);
    fn write_raw(spi_id: u8, data: &[u8]);
    fn transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8]);

    /// JVM-array-addressed counterparts of the `_raw` pair — the
    /// `picodroid.pio.SpiDevice` entry points, defaulted over them through
    /// [`crate::hal::array_io`] for the reason [`HalI2c::write`] gives.
    /// `transfer` returns `len`, `0` for an empty transfer, or `-1` past the
    /// cap; `write` the same, with nothing read back.
    fn transfer(spi_id: u8, tx_idx: u16, rx_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
        crate::hal::array_io::spi_transfer(
            Self::JAVA_XFER_MAX,
            arrays,
            tx_idx,
            rx_idx,
            len,
            |tx, rx| Self::transfer_raw(spi_id, tx, rx),
        )
    }
    fn write(spi_id: u8, data_idx: u16, len: usize, arrays: &ArrayHeap) -> i32 {
        crate::hal::array_io::spi_write(Self::JAVA_XFER_MAX, arrays, data_idx, len, |data| {
            Self::write_raw(spi_id, data)
        })
    }
}

pub trait HalUart {
    fn init(uart_id: u8);
    fn write_byte(uart_id: u8, byte: u8);
    fn read_byte(uart_id: u8) -> i32;
    /// Apply a full line configuration at once. Each `picodroid.pio.Uart`
    /// setter re-sends every field, so there is no partial-update path for a
    /// platform to get wrong.
    fn reconfigure(
        uart_id: u8,
        baudrate: i32,
        data_size: i32,
        parity: i32,
        stop_bits: i32,
        hw_flow: i32,
    );
}

/// TCP/UDP sockets and link status. Required only when `cfg(has_network)`.
///
/// Sockets stay opaque `*mut c_void` handles owned by the platform stack —
/// shared code only ever passes them back.
pub trait HalNet {
    fn tcp_socket() -> Result<*mut c_void, NetError>;
    fn tcp_connect(sock: *mut c_void, addr: u32, port: u16) -> Result<(), NetError>;
    fn tcp_send(sock: *mut c_void, data: &[u8]) -> Result<usize, NetError>;
    fn tcp_recv(sock: *mut c_void, buf: &mut [u8]) -> Result<usize, NetError>;
    fn tcp_listen(sock: *mut c_void, port: u16) -> Result<(), NetError>;
    fn tcp_accept(sock: *mut c_void) -> Result<*mut c_void, NetError>;
    fn udp_socket(local_port: u16) -> Result<*mut c_void, NetError>;
    fn udp_sendto(sock: *mut c_void, buf: &[u8], addr: u32, port: u16) -> Result<usize, NetError>;
    /// Blocking receive. Returns `(bytes_read, source_addr, source_port)`.
    fn udp_recvfrom(sock: *mut c_void, buf: &mut [u8]) -> Result<(usize, u32, u16), NetError>;
    fn close(sock: *mut c_void);
    fn set_recv_timeout(sock: *mut c_void, ms: u32);
    fn is_network_up() -> bool;
    fn get_ip_address() -> u32;
    fn dns_resolve(hostname: &str) -> Result<u32, NetError>;
}

/// Persistent storage behind `picodroid.io.File` and friends.
///
/// Deliberately path-in / value-out with no handle type: LittleFS on the
/// device wants a borrow of its filesystem for the duration of each
/// operation (`with_fs(|fs| …)`), and a `File` handle crossing the seam
/// would either outlive that borrow or force the platform to keep a handle
/// table. Every call re-resolves the path instead, which is what the natives
/// did before the extraction.
///
/// Failures are folded into the return value the same way the Java API
/// reports them — `false`, `0`, or `-1` — because `java.io.File`'s
/// predicates cannot throw.
pub trait HalFs {
    fn exists(path: &str) -> bool;
    fn is_file(path: &str) -> bool;
    fn is_dir(path: &str) -> bool;
    /// Size in bytes, or 0 if absent — matching `File.length()`.
    fn length(path: &str) -> i64;
    fn delete(path: &str) -> bool;
    fn mkdir(path: &str) -> bool;
    fn rename(from: &str, to: &str) -> bool;
    fn truncate(path: &str);
    /// Append up to `len` bytes from `pos` onto `out`. Returns bytes read,
    /// 0 at EOF, or -1 on error.
    fn read_at(path: &str, pos: u64, out: &mut alloc::vec::Vec<u8>, len: usize) -> i32;
    /// Returns bytes written, or -1 on error. Creates the file if absent.
    fn write_at(path: &str, pos: u64, data: &[u8]) -> i32;
}
