// SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(not(any(test, feature = "sim")), no_std)]

extern crate alloc;

// Background-pool worker loop. Deliberately in this crate: it drives a `Jvm`,
// and a second JVM-driving crate duplicates the whole interpreter (see the
// module docs). Device and simulator both run it — the simulator spawns the
// same four worker tasks the device does (`sim_boot.rs`).
#[cfg(not(test))]
pub mod bg_worker;
pub mod board_cfg;
// App startup: shared heap, class loaders, `run_app`. Needs the JVM natives
// and the whole graphics tree, hence `cfg(not(test))` like they are.
#[cfg(not(test))]
pub mod boot;
pub mod dispatch_sites;
pub mod drivers;
pub mod executors;
pub mod framework_classes;
// LittleFS, opt-in per family. See the feature's note in Cargo.toml.
#[cfg(all(feature = "littlefs", not(test)))]
pub mod fs;
// Build artifact, no JVM/graphics deps — see the module docs for why it is
// not inside `boot`.
pub mod framework_map;
pub mod gc_root_registration;
pub mod gc_roots;
pub mod graphics;
pub mod hal;
// Compiled under test: hardware/sensors/mailbox.rs is pure atomics and
// carries host unit tests (the JVM-facing natives inside stay
// `cfg(not(test))`).
pub mod hardware;
pub mod host;
// Always compiled, including under `cfg(test)`: the whole point of moving the
// install path here is that it finally runs in a test binary. It was gated
// out of every host build while it lived in the family crate, so the one code
// path that can leave a device unbootable had no coverage at all.
pub mod install;
// Synthetic input gestures, shared by the debug bridge and the simulator's
// control channel so both answer a script identically.
pub mod input_inject;
#[cfg(not(test))]
pub mod lifecycle;
pub mod lvgl_ffi;
#[cfg(all(feature = "mem-diag", not(test)))]
pub mod mem_diag;
pub mod monitor_store;
// The JVM's native dispatch. `cfg(not(test))` because its submodules reach
// the HAL, LVGL and the lifecycle; the pure-logic ones are re-exposed as
// test shims at the bottom of this file so their checks still run.
#[cfg(not(test))]
pub mod native_handler;
// Board-gated: `has_network` comes from board.toml via this crate's build.rs.
#[cfg(all(not(test), has_network))]
pub mod net;
#[cfg(not(test))]
pub mod notification;
// `cfg(not(test))` for the same reason it carried that gate in the binary
// crate: these reach the JVM natives and the HAL, not pure logic.
#[cfg(not(test))]
pub mod os;
pub mod pd_log;
// Debug bridge protocol. Always compiled: it is a wire format with a host
// counterpart, so its framing and encoders are exactly what wants tests.
pub mod pdb;
// Ungated: `peripheral_manager`'s ref-name parsing is pure logic and carries
// host unit tests.
pub mod pio;
pub mod rtos;
#[cfg(not(test))]
pub mod service_lifecycle;
pub mod shrink_names;
// Simulator boot topology — the host's `boot_tasks.rs`. Family policy
// arrives as `BootLeaves`; see the module docs and
// docs/designs/family-neutral-residue.md B11.
#[cfg(all(feature = "sim", not(test)))]
pub mod sim_boot;
pub mod task_priority;
#[cfg(not(test))]
pub mod util;

#[cfg(test)]
mod test_platform;

// Pure-logic submodules of `native_handler`, re-exposed so their tests run
// under `scripts/test.sh` despite the parent being `cfg(not(test))`. These
// shims came across with the module; without them the checks below compile
// but never execute, which is the silent-no-coverage trap this repo has hit
// before.
//
// Activity stack + pending-op FIFO — hardware-free state machine.
#[cfg(test)]
#[path = "native_handler/state.rs"]
mod native_handler_state_tests;
// Native-class registry + its cross-check: every SDK class declaring a
// `native` method must appear in the registry, or virtual dispatch fails at
// runtime with NoSuchMethod.
#[cfg(test)]
#[path = "native_handler/class_registry.rs"]
mod native_class_registry_tests;
// Method-level counterpart: every SDK `native` method must have a dispatch
// arm, and every declared arm must match a real SDK declaration.
#[cfg(test)]
#[path = "native_handler/method_tables.rs"]
mod native_method_tables_tests;
