// SPDX-License-Identifier: GPL-3.0-only
//! Logging that works in every build of this crate.
//!
//! Framework code in the binary crate logs through paired arms:
//!
//! ```ignore
//! #[cfg(not(feature = "sim"))] defmt::info!("...");
//! #[cfg(feature = "sim")]      println!("[Tag] ...");
//! ```
//!
//! That pairing does not survive the move, because it assumes `not(sim)`
//! implies "on the device". This crate also builds for the host without the
//! `sim` feature, where `println!` is unavailable under `no_std`.
//!
//! So the split is on **whether `std` is linked**, using the same condition
//! as this crate's own `no_std` attribute — `any(test, feature = "sim")`.
//! One rule, no third case to reason about:
//!
//! | Build | Arm |
//! |---|---|
//! | device firmware | defmt |
//! | simulator | `eprintln!` |
//! | `cargo test -p picodroid-core` | `eprintln!` |
//! | bare `cargo build -p picodroid-core` | defmt (compiles under `no_std`; no link step) |
//!
//! Moved code converts its paired arms to these macros, which is
//! net-negative on lines since one call replaces two cfg-gated ones.
//!
//! # Output format
//!
//! Device logs read `Tag: message` (defmt's framing); the simulator emits
//! `[Tag] message`. The nightly log-diffing harness already normalises
//! between the two, so keep passing the tag exactly as today's call sites do.
//!
//! # Format specifiers
//!
//! defmt's `{=u8}`-style type hints are accepted on the defmt arm only. Use
//! plain `{}` in shared code so both arms compile.

/// Trace-level log. See the [module docs](self) for arm selection.
#[macro_export]
macro_rules! pd_trace {
    ($($arg:tt)*) => {{
        #[cfg(not(any(test, feature = "sim")))]
        { ::defmt::trace!($($arg)*); }
        #[cfg(any(test, feature = "sim"))]
        { ::std::eprintln!($($arg)*); }
    }};
}

/// Debug-level log. See the [module docs](self) for arm selection.
#[macro_export]
macro_rules! pd_debug {
    ($($arg:tt)*) => {{
        #[cfg(not(any(test, feature = "sim")))]
        { ::defmt::debug!($($arg)*); }
        #[cfg(any(test, feature = "sim"))]
        { ::std::eprintln!($($arg)*); }
    }};
}

/// Info-level log. See the [module docs](self) for arm selection.
#[macro_export]
macro_rules! pd_info {
    ($($arg:tt)*) => {{
        #[cfg(not(any(test, feature = "sim")))]
        { ::defmt::info!($($arg)*); }
        #[cfg(any(test, feature = "sim"))]
        { ::std::eprintln!($($arg)*); }
    }};
}

/// Warn-level log. See the [module docs](self) for arm selection.
#[macro_export]
macro_rules! pd_warn {
    ($($arg:tt)*) => {{
        #[cfg(not(any(test, feature = "sim")))]
        { ::defmt::warn!($($arg)*); }
        #[cfg(any(test, feature = "sim"))]
        { ::std::eprintln!($($arg)*); }
    }};
}

/// Error-level log. See the [module docs](self) for arm selection.
#[macro_export]
macro_rules! pd_error {
    ($($arg:tt)*) => {{
        #[cfg(not(any(test, feature = "sim")))]
        { ::defmt::error!($($arg)*); }
        #[cfg(any(test, feature = "sim"))]
        { ::std::eprintln!($($arg)*); }
    }};
}
