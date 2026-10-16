// SPDX-License-Identifier: GPL-3.0-only
//! One initialiser per bus, by construction.
//!
//! A bus's `init` used to be check-then-store: read "is the lock cell
//! filled", and if not, build the semaphores, program the block and fill
//! the cells. Two tasks arriving together would both see an empty cell and
//! both initialise: two semaphores, the block programmed twice, and a
//! `SetOnce::set` racing a reader. The callers never overlapped (board
//! bring-up before the scheduler, then `PeripheralManager.openSpi` or an
//! `I2cDevice` from Java), so it never fired; but that was timing, not
//! structure.
//!
//! [`InitGate`] makes the first arrival the only initialiser. The claim is
//! one load and one store under masked interrupts, so exactly one caller
//! gets [`Claim::Ours`]. A caller that arrives while the initialiser is
//! still running gets [`Claim::Busy`] and waits for [`InitGate::finish`]
//! on the RTOS; one that arrives after gets [`Claim::Done`] and returns.
//!
//! Every bus initialiser runs on core 0 (board bring-up, the JVM tasks and
//! the sensor sampler all do; `task_affinity` pins only the flash parker
//! and the cyw43 driver to core 1), so `cortex_m::interrupt::free` is
//! enough for the claim. It is the section `periph::enable_irq` uses too.

use core::sync::atomic::{AtomicU8, Ordering};

const EMPTY: u8 = 0;
const BUSY: u8 = 1;
const DONE: u8 = 2;

/// The initialisation state of one bus. See the module docs.
pub struct InitGate(AtomicU8);

/// What [`InitGate::claim`] gave the caller.
#[must_use]
pub enum Claim {
    /// This caller is the initialiser: run the init, then
    /// [`InitGate::finish`].
    Ours,
    /// Another task is initialising right now: [`InitGate::wait_done`].
    Busy,
    /// Initialised already.
    Done,
}

impl InitGate {
    pub const fn new() -> Self {
        Self(AtomicU8::new(EMPTY))
    }

    /// True once the initialiser called [`finish`](Self::finish).
    pub fn is_done(&self) -> bool {
        self.0.load(Ordering::Acquire) == DONE
    }

    /// Claim the right to initialise. Exactly one caller ever gets
    /// [`Claim::Ours`].
    pub fn claim(&self) -> Claim {
        cortex_m::interrupt::free(|_| match self.0.load(Ordering::Relaxed) {
            EMPTY => {
                self.0.store(BUSY, Ordering::Relaxed);
                Claim::Ours
            }
            BUSY => Claim::Busy,
            _ => Claim::Done,
        })
    }

    /// The initialiser is finished: everything it set up is visible to a
    /// task that sees [`is_done`](Self::is_done).
    pub fn finish(&self) {
        self.0.store(DONE, Ordering::Release);
    }

    /// Block until the initialiser finishes. Only a [`Claim::Busy`] caller
    /// gets here, and two callers at once means two tasks, so the scheduler
    /// is running and the wait is an RTOS delay, not a spin. Never taken
    /// today: no two bus initialisers overlap.
    pub fn wait_done(&self) {
        while !self.is_done() {
            freertos_rust::CurrentTask::delay(freertos_rust::Duration::ms(1));
        }
    }
}
