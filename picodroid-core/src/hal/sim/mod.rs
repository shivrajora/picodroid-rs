// SPDX-License-Identifier: GPL-3.0-only
//! Simulator HAL — the host implementation of the hardware surface.
//!
//! This lives in `picodroid-core`, not in a platform crate, and that is the
//! point. Every family used to be told to copy `hal/sim/` into its own tree;
//! the ESP scaffold did exactly that and accumulated 17 stub modules that
//! drifted from the originals before it was removed. A simulator that is
//! shared cannot drift.
//!
//! `pdb_usb` is a stub for machinery no simulator can stand in for (the USB
//! debug bridge). There are deliberately no `boot` / `flash` stubs beside
//! it: an empty `clock_init` and two flash constants that no simulator build
//! could reach were shape parity and nothing more, so `crate::hal`'s
//! re-exports of those two are gated to device builds instead. A family
//! whose simulator models a real flash region defines its own module.
//!
//! Sibling modules here call each other directly (`super::gpio::inject`)
//! rather than through [`crate::hal`]'s facade: the facade would route back
//! out through the platform's registration and straight into these same
//! functions, which is a round-trip that buys nothing.

pub mod adc;
pub mod allocator;
// The boot-budget engine: charges the arena from a family's model of the
// device's boot-time tasks (`register_sim_platform!`'s `boot_budget`).
pub mod boot_budget;
pub mod delay;
pub mod display;
// `pvPortMalloc` and friends for the hosted kernel, which is compiled without
// a `heap_N.c`. Present in every build that links it, including this crate's
// test build — the kernel is linked there too, it simply never runs.
pub mod freertos_heap_shim;
pub mod gpio;
pub mod heap4;
pub mod i2c;
pub mod input_pin;
pub mod output_pin;
pub mod pdb_usb;
pub mod platform;
pub mod pwm;
// `hal::sim::rtos` is the real FreeRTOS kernel — every simulator *run* gets
// the device's scheduler, not a model of it.
//
// `cargo test` is the exception, and it is a constraint rather than a
// preference. The harness runs cases concurrently on threads it owns, with no
// scheduler ever started; the kernel's task APIs then dereference a "current
// task" that does not exist and the process dies (measured: 198 tests in, then
// SIGSEGV). So this crate's own test build keeps [`rtos_std`] — host threads,
// condvars and a hand-tracked recursive mutex — which is exactly what
// `test_platform.rs` exercises. Note that `cfg(test)` is true only when *this*
// crate is the test target; a platform crate's tests build this one normally
// and get the kernel backing, which is fine because they never spawn a task.
//
// Both expose the same free functions, so `register_sim_platform!` and
// `test_platform.rs` name `rtos::` and never the choice. The selection is made
// here rather than inside the macro: a `#[cfg]` written in a macro body is
// evaluated against the expanding crate, not this one.
#[cfg(test)]
#[path = "rtos.rs"]
pub mod rtos_std;
#[cfg(test)]
pub use rtos_std as rtos;
#[cfg(not(test))]
pub mod rtos_freertos;
#[cfg(not(test))]
pub use rtos_freertos as rtos;
pub mod spi;
pub mod spi_bus;
pub mod system_clock;
pub mod touch;
pub mod uart;

#[cfg(has_network)]
pub mod net;
