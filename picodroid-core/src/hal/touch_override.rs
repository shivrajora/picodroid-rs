// SPDX-License-Identifier: GPL-3.0-only
//! Scripted touch, shared by every family.
//!
//! A debug bridge (`pdb input tap` / `swipe`) or the simulator's control
//! channel wants an injected touch to run the exact same read → LVGL
//! hit-test → Java `MotionEvent` path as a real finger. The way to get that is
//! not to synthesise events downstream but to make the panel *read* the
//! scripted point: while the override is engaged, whatever samples the panel
//! reports the scripted position instead. The RP family and the simulator
//! each carried these three atomics and this state machine; this is it once
//! (`docs/designs/porting-seam-2026-09.md` E2/H3).
//!
//! # States
//!
//! `Inactive` → [`inject`](TouchOverride::inject) → `Pressed(x, y)` →
//! [`release`](TouchOverride::release) → `Lifted(x, y)` →
//! [`clear`](TouchOverride::clear) → `Inactive`. `Lifted` keeps reporting the
//! last position, not pressed, so the reader observes a RELEASE edge from the
//! scripted point before real sampling resumes; another `inject` from
//! `Lifted` presses again (a drag that lifts and re-touches).
//!
//! Plain `Relaxed` atomics: the writer is another task or thread and the
//! reader polls at tick rate, so a stale sample costs one tick and nothing
//! else. The position is packed into one word so a move is one store.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// What the scripted touch currently says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverrideSample {
    /// Not engaged — sample the real panel (or the host mouse).
    Inactive,
    /// Engaged and pressed at `(x, y)`.
    Pressed(u16, u16),
    /// Engaged and lifted: report "not pressed" at the last scripted
    /// position until [`TouchOverride::clear`].
    Lifted(u16, u16),
}

pub struct TouchOverride {
    active: AtomicBool,
    pressed: AtomicBool,
    /// `(x << 16) | y`.
    pos: AtomicU32,
}

impl TouchOverride {
    pub const fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            pressed: AtomicBool::new(false),
            pos: AtomicU32::new(0),
        }
    }

    /// Begin, or continue, a scripted touch at `(x, y)` — a press, or a
    /// drag-move while pressed.
    pub fn inject(&self, x: u16, y: u16) {
        self.pos
            .store(((x as u32) << 16) | y as u32, Ordering::Relaxed);
        self.pressed.store(true, Ordering::Relaxed);
        self.active.store(true, Ordering::Relaxed);
    }

    /// Lift the scripted touch but stay engaged, so the RELEASE edge is
    /// observed from the scripted position before real sampling resumes.
    pub fn release(&self) {
        self.pressed.store(false, Ordering::Relaxed);
    }

    /// Disengage entirely; the reader returns to the real panel.
    pub fn clear(&self) {
        self.pressed.store(false, Ordering::Relaxed);
        self.active.store(false, Ordering::Relaxed);
    }

    pub fn sample(&self) -> OverrideSample {
        if !self.active.load(Ordering::Relaxed) {
            return OverrideSample::Inactive;
        }
        let packed = self.pos.load(Ordering::Relaxed);
        let (x, y) = ((packed >> 16) as u16, (packed & 0xFFFF) as u16);
        if self.pressed.load(Ordering::Relaxed) {
            OverrideSample::Pressed(x, y)
        } else {
            OverrideSample::Lifted(x, y)
        }
    }
}

impl Default for TouchOverride {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_the_state_machine() {
        let o = TouchOverride::new();
        assert_eq!(o.sample(), OverrideSample::Inactive);
        o.inject(12, 340);
        assert_eq!(o.sample(), OverrideSample::Pressed(12, 340));
        o.inject(13, 341); // drag-move
        assert_eq!(o.sample(), OverrideSample::Pressed(13, 341));
        o.release();
        assert_eq!(o.sample(), OverrideSample::Lifted(13, 341));
        o.clear();
        assert_eq!(o.sample(), OverrideSample::Inactive);
    }

    #[test]
    fn inject_after_release_presses_again() {
        let o = TouchOverride::new();
        o.inject(1, 2);
        o.release();
        o.inject(3, 4);
        assert_eq!(o.sample(), OverrideSample::Pressed(3, 4));
    }

    #[test]
    fn release_without_inject_stays_inactive() {
        let o = TouchOverride::new();
        o.release();
        assert_eq!(o.sample(), OverrideSample::Inactive);
    }

    #[test]
    fn full_range_coordinates_survive_the_packing() {
        let o = TouchOverride::new();
        o.inject(u16::MAX, 0);
        assert_eq!(o.sample(), OverrideSample::Pressed(u16::MAX, 0));
        o.inject(0, u16::MAX);
        assert_eq!(o.sample(), OverrideSample::Pressed(0, u16::MAX));
        o.inject(0x1234, 0xABCD);
        assert_eq!(o.sample(), OverrideSample::Pressed(0x1234, 0xABCD));
    }
}
