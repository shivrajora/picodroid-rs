// SPDX-License-Identifier: GPL-3.0-only
//! Native backing for `picodroid.media` — tones on a board's sound output.
//!
//! Gated per board by the `[audio]` section in board.toml (`cfg(has_audio)`).
//! A board that declares none also drops the SDK classes from its embedded
//! framework (`build_support/board_cfg.rs::AUDIO_CLASSES`), so on a board that
//! cannot make a noise `picodroid.media` costs nothing at all — no flash, no
//! RAM, and an app naming it fails at compile time rather than at runtime.
//!
//! The split here is deliberate. [`tone_table`] and [`sequencer`] are pure data
//! and arithmetic with no HAL beneath them, so they compile and run their tests
//! on the host like [`crate::json`] does, with no `#[path]` shim needed. Only
//! [`driver`] touches hardware, and it is thin: it owns the one sequencer, the
//! volume, and the arithmetic that turns a frequency into a PWM configuration.
//!
//! What the hardware can do is the whole design constraint, and it is worth
//! stating once: the only sound output picodroid supports is a piezo buzzer on
//! a PWM pad. One square wave at a time, no DAC, no I2S, no amplifier. Tones
//! and melodies work; sampled audio has no path at all, which is why
//! `MediaPlayer`, `AudioTrack` and `SoundPool` are absent rather than stubbed.

// `test` forces these on so their checks run on the host whatever board the
// workspace test lane happens to select — the same idiom `drivers/mod.rs` uses
// for a panel driver.
#[cfg(any(has_audio, test))]
pub mod sequencer;
#[cfg(any(has_audio, test))]
pub mod tone_table;

// Reaches the HAL, so it follows the same `cfg(not(test))` rule the rest of the
// crate's hardware-facing code does. The pure halves above stay visible to the
// host tests.
#[cfg(all(has_audio, not(test)))]
mod driver;
#[cfg(all(has_audio, not(test)))]
pub mod natives;
#[cfg(all(has_audio, not(test)))]
pub use driver::{on_tick, stop};
