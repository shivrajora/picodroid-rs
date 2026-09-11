// SPDX-License-Identifier: GPL-3.0-only
//! The one buzzer: sequencer state, volume, and the PWM writes.
//!
//! There is exactly one sound output on a board, so there is one sequencer
//! here rather than one per `ToneGenerator`. Starting a tone replaces whatever
//! was playing, and the volume in effect is the one from the most recently
//! constructed generator. Android mixes per-stream and would not do either; a
//! single piezo leaves no honest alternative, and `ToneGenerator`'s Javadoc
//! says so.
//!
//! Segments advance on the 16 ms UI tick (`lifecycle.rs`), not on a task of
//! their own — no extra stack, no `boot_budget.rs` row, and no scheduler work
//! for a feature that is idle almost always. The sequencer only speaks at a
//! segment boundary, so a tick that changes nothing costs one comparison and
//! the hardware is touched once per note.

use core::sync::atomic::{AtomicBool, Ordering};
use pico_jvm::atomic_section::AtomicSection;

use super::sequencer::{Action, Sequencer, MAX_SEGMENTS};
use super::tone_table::{descriptor, Segment};
use crate::hal::pwm;

include!(concat!(env!("OUT_DIR"), "/audio_config.rs"));

/// Duty cycle at full volume, as a percentage.
///
/// A square wave carries the most energy at 50%, and past it the waveform is
/// only the mirror image of one below it, so 50% is genuinely the loudest this
/// can be rather than an arbitrary cap.
const MAX_DUTY_PERCENT: f64 = 50.0;

/// Frequency handed to the PWM block when the output is being silenced.
///
/// The block wants some frequency even with the channel disabled, and the
/// divisor arithmetic clamps at 1 Hz, so this is a plain placeholder: at 0%
/// duty and disabled, nothing about it is audible.
const IDLE_FREQ_HZ: f64 = 1000.0;

/// The sequencer and the volume.
///
/// `static mut` rather than atomics because the state is a compound the
/// sequencer mutates in several steps, and because the RP2040's Cortex-M0+ has
/// no `compare_exchange` to build one from. Every access goes through
/// [`with_state`]: `on_tick` runs on the JVM main task while `start_tone` may
/// be called from any Java thread, so the two really do race.
static mut SEQ: Sequencer = Sequencer::new();
static mut VOLUME: u8 = 0;

/// Whether a tone is playing, published for [`on_tick`]'s fast path.
///
/// This exists so the tick can answer "nothing to do" without touching the
/// scheduler — see [`on_tick`]. It is written only inside [`with_state`], from
/// the sequencer's own state, so it cannot drift from it. A plain load and
/// store, never a read-modify-write, because the Cortex-M0+ has no CAS.
static TONE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Whether the PWM pad has been configured yet.
static PIN_READY: AtomicBool = AtomicBool::new(false);

#[allow(static_mut_refs)]
fn with_state<R>(f: impl FnOnce(&mut Sequencer, &mut u8) -> R) -> R {
    let _atomic = AtomicSection::enter();
    // SAFETY: the atomic section is the mutual exclusion; see the statics' docs.
    let r = unsafe { f(&mut SEQ, &mut VOLUME) };
    // SAFETY: as above — still inside the section.
    unsafe { TONE_ACTIVE.store(SEQ.is_active(), Ordering::Relaxed) };
    r
}

fn now_ms() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

/// Configure the pad, once, the first time a generator is constructed. Doing
/// it lazily rather than at boot keeps a board that never makes a sound from
/// claiming a PWM slice it does not use.
///
/// No atomic section: two threads racing here would both call `pwm::init`,
/// which is idempotent by construction (it re-runs the same resets and pad
/// writes), so the benign race is cheaper than suspending the scheduler.
fn ensure_pin() {
    if PIN_READY.load(Ordering::Relaxed) {
        return;
    }
    PIN_READY.store(true, Ordering::Relaxed);
    pwm::init(AUDIO_PIN);
}

/// Apply an [`Action`] to the hardware. Called outside the atomic section: the
/// PWM writes are slow-ish register work and nothing else drives this pad.
fn apply(action: Action, volume: u8) {
    match action {
        Action::Play(0) | Action::Done => pwm::apply(AUDIO_PIN, IDLE_FREQ_HZ, 0.0, false),
        Action::Play(freq) => {
            let duty = MAX_DUTY_PERCENT * (volume as f64) / 100.0;
            pwm::apply(AUDIO_PIN, freq as f64, duty, true)
        }
    }
}

/// `ToneGenerator.nativeInit` — remember the volume and claim the pad.
///
/// `streamType` is not passed: there is no mixer for it to select and the Java
/// side documents it as ignored.
pub fn init(volume: i32) {
    ensure_pin();
    let v = volume.clamp(0, 100) as u8;
    with_state(|_, vol| *vol = v);
}

/// `ToneGenerator.startTone`. False for a tone the table does not define,
/// which is what Android reports for a tone its platform lacks.
pub fn start_tone(tone_type: i32, duration_ms: i32) -> bool {
    let Some(tone) = descriptor(tone_type) else {
        return false;
    };
    ensure_pin();
    let cap = duration_ms.max(0) as u32;
    let now = now_ms();
    let started = with_state(|seq, vol| seq.start_tone(&tone, now, cap).map(|a| (a, *vol)));
    match started {
        Some((action, volume)) => {
            apply(action, volume);
            true
        }
        None => false,
    }
}

/// `ToneGenerator.startToneSequence`, the picodroid extension.
///
/// The notes are copied into the sequencer before this returns, so the caller's
/// arrays are free immediately and native code holds no JVM reference across a
/// collection.
pub fn start_sequence(freqs: &[i32], durations: &[i32]) -> bool {
    if freqs.len() != durations.len() || freqs.is_empty() || freqs.len() > MAX_SEGMENTS {
        return false;
    }
    let mut segments = [Segment::new(0, 0); MAX_SEGMENTS];
    for (i, seg) in segments.iter_mut().take(freqs.len()).enumerate() {
        // A note out of range is clamped rather than refused: an app asking for
        // 40 kHz has a bug, but silencing the whole melody teaches it less than
        // hearing the note come out wrong.
        *seg = Segment::new(
            freqs[i].clamp(0, u16::MAX as i32) as u16,
            durations[i].clamp(0, u16::MAX as i32) as u16,
        );
    }
    ensure_pin();
    let now = now_ms();
    let started = with_state(|seq, vol| {
        seq.start_sequence(&segments[..freqs.len()], now, 0)
            .map(|a| (a, *vol))
    });
    match started {
        Some((action, volume)) => {
            apply(action, volume);
            true
        }
        None => false,
    }
}

/// `ToneGenerator.stopTone`.
pub fn stop() {
    let was_active = with_state(|seq, _| {
        let active = seq.is_active();
        seq.stop();
        active
    });
    if was_active {
        apply(Action::Done, 0);
    }
}

/// `ToneGenerator.release` — stop, and leave the pad idle rather than holding
/// a level across it.
pub fn release() {
    stop();
}

/// Advance the tone from the UI tick.
///
/// The first line is load-bearing, not an optimisation. This runs on every UI
/// frame, about sixty times a second, for the whole life of the device, and
/// silence is the overwhelmingly common case. [`AtomicSection`] is
/// `vTaskSuspendAll`/`xTaskResumeAll` — taking one unconditionally suspends the
/// scheduler on every frame forever, which starves the USB device task: on
/// hardware the board kept running and kept logging over RTT while its USB CDC
/// interface silently vanished from the host. Reading a flag costs one load and
/// touches the scheduler not at all.
pub fn on_tick() {
    if !TONE_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let now = now_ms();
    let stepped = with_state(|seq, vol| seq.advance(now).map(|a| (a, *vol)));
    if let Some((action, volume)) = stepped {
        apply(action, volume);
    }
}
