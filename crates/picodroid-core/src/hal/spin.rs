// SPDX-License-Identifier: GPL-3.0-only
//! The one sanctioned way to spin on hardware state.
//!
//! A handful of waits in the HAL have to be spins: a DMA channel retiring an
//! abort, a peripheral coming out of reset, the flash parker's cross-core
//! handshake (the awaited task runs on the *other* core, so blocking here
//! would help nothing). What they must not be is unbounded — a wedged
//! channel or a parker that never ran then hangs the calling task silently.
//! [`spin_until!`] gives every such wait a name and an iteration cap, and
//! returns [`SpinTimeout`] instead of looping forever. The source guard in
//! `platforms/rp/src/spin_guard.rs` accepts a spin written with this macro
//! and rejects a bare `while … {}` register poll, so the cap cannot be left
//! off by accident (docs/scheduling-audit-2026-09.md, G5).
//!
//! Anything that waits a millisecond or more belongs on the RTOS — a
//! semaphore an interrupt gives, a task notification, or `rtos::delay_ms`
//! — not here.

/// A [`spin_until!`] wait that hit its iteration cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpinTimeout {
    /// The `name` the macro was given — what was being waited for.
    pub name: &'static str,
    /// Iterations spent before giving up (the cap plus one).
    pub iters: u32,
}

/// Spin until `$cond` holds, at most `$max` iterations.
///
/// Evaluates to `Result<(), SpinTimeout>`. Each iteration executes one
/// `spin_loop` hint (a no-op on thumbv6m, a `yield` on cores that have
/// one), so `$max` is a count of condition reads, not a duration; size it
/// generously against the expected wait (the memory-bus reads dominate) and
/// treat a timeout as a fault to report, never as a cadence to tune.
#[macro_export]
macro_rules! spin_until {
    ($cond:expr, $max:expr, $name:literal) => {{
        let mut __spins: u32 = 0;
        loop {
            if $cond {
                break Ok(());
            }
            __spins += 1;
            if __spins > $max {
                break Err($crate::hal::spin::SpinTimeout {
                    name: $name,
                    iters: __spins,
                });
            }
            core::hint::spin_loop();
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::SpinTimeout;

    #[test]
    fn satisfied_condition_returns_ok_without_spinning() {
        assert_eq!(spin_until!(true, 0, "already"), Ok(()));
    }

    #[test]
    fn condition_that_becomes_true_returns_ok() {
        let mut n = 0;
        let r = spin_until!(
            {
                n += 1;
                n == 5
            },
            10,
            "fifth read"
        );
        assert_eq!(r, Ok(()));
        assert_eq!(n, 5);
    }

    #[test]
    fn cap_is_a_hard_bound_and_names_the_wait() {
        let r = spin_until!(false, 3, "never");
        assert_eq!(
            r,
            Err(SpinTimeout {
                name: "never",
                iters: 4
            })
        );
    }
}
