// SPDX-License-Identifier: GPL-3.0-only
//! Lock-free mailbox + control plane between the sensor sampler (FreeRTOS
//! task on device, `std::thread` on sim) and the JVM main task.
//!
//! Data plane: one seqlock cell per sensor cluster, written only by the
//! sampler, read only by the JVM task's drain. `seq == 0` means "never
//! published" (doubles as the validity flag that preserves the
//! no-zero-deliveries-before-first-conversion behavior); an odd `seq`
//! means a write is in progress. The reader NEVER spins: on a torn read it
//! returns `None` and the drain retries next tick — on device the reader
//! outprioritizes the writer on the same core, so a spinning reader could
//! livelock the writer's odd→even transition.
//!
//! Control plane: single-writer (JVM task) atomics the sampler re-reads on
//! every loop turn, so a momentarily-inconsistent enable/period pair
//! converges within one wake.
//!
//! Everything here is plain atomic load/store — no CAS (RP2040's Cortex-M0+
//! has none), no kernel objects — which is what lets one implementation
//! serve device, sim, and host unit tests. The fence protocol mirrors
//! crossbeam's SeqLock (odd store, Release fence, field stores, even
//! Release store / Acquire load, field loads, Acquire fence, re-check).

use core::sync::atomic::{fence, AtomicBool, AtomicI32, AtomicU32, Ordering};

use super::{EnvSnapshot, OpticalSnapshot};

// ── Data plane ───────────────────────────────────────────────────────────────

struct EnvCell {
    seq: AtomicU32,
    temp_centi_c: AtomicI32,
    hum_milli_pct: AtomicU32,
    press_pa: AtomicU32,
    gas_ohm: AtomicU32,
}

struct OpticalCell {
    seq: AtomicU32,
    lux_milli: AtomicU32,
    proximity_raw: AtomicU32, // u16 payload widened to the atomic word
}

static ENV: EnvCell = EnvCell {
    seq: AtomicU32::new(0),
    temp_centi_c: AtomicI32::new(0),
    hum_milli_pct: AtomicU32::new(0),
    press_pa: AtomicU32::new(0),
    gas_ohm: AtomicU32::new(0),
};

static OPT: OpticalCell = OpticalCell {
    seq: AtomicU32::new(0),
    lux_milli: AtomicU32::new(0),
    proximity_raw: AtomicU32::new(0),
};

/// Publish a BME688 snapshot. Sampler-side only (single writer).
pub fn publish_env(s: &EnvSnapshot) {
    let seq = ENV.seq.load(Ordering::Relaxed);
    ENV.seq.store(seq.wrapping_add(1), Ordering::Relaxed); // odd: write begins
    fence(Ordering::Release);
    ENV.temp_centi_c.store(s.temp_centi_c, Ordering::Relaxed);
    ENV.hum_milli_pct.store(s.hum_milli_pct, Ordering::Relaxed);
    ENV.press_pa.store(s.press_pa, Ordering::Relaxed);
    ENV.gas_ohm.store(s.gas_ohm, Ordering::Relaxed);
    ENV.seq.store(seq.wrapping_add(2), Ordering::Release); // even: write ends
}

/// Publish an LTR559 snapshot. Sampler-side only (single writer).
pub fn publish_optical(s: &OpticalSnapshot) {
    let seq = OPT.seq.load(Ordering::Relaxed);
    OPT.seq.store(seq.wrapping_add(1), Ordering::Relaxed);
    fence(Ordering::Release);
    OPT.lux_milli.store(s.lux_milli, Ordering::Relaxed);
    OPT.proximity_raw
        .store(s.proximity_raw as u32, Ordering::Relaxed);
    OPT.seq.store(seq.wrapping_add(2), Ordering::Release);
}

/// Latest BME688 snapshot, or `None` if nothing was ever published or the
/// single read attempt raced a publish (drain retries next tick).
pub fn read_env() -> Option<EnvSnapshot> {
    let s1 = ENV.seq.load(Ordering::Acquire);
    if s1 == 0 || s1 & 1 == 1 {
        return None;
    }
    let snap = EnvSnapshot {
        temp_centi_c: ENV.temp_centi_c.load(Ordering::Relaxed),
        hum_milli_pct: ENV.hum_milli_pct.load(Ordering::Relaxed),
        press_pa: ENV.press_pa.load(Ordering::Relaxed),
        gas_ohm: ENV.gas_ohm.load(Ordering::Relaxed),
    };
    fence(Ordering::Acquire);
    let s2 = ENV.seq.load(Ordering::Relaxed);
    (s1 == s2).then_some(snap)
}

/// Latest LTR559 snapshot, or `None` (see [`read_env`]).
pub fn read_optical() -> Option<OpticalSnapshot> {
    let s1 = OPT.seq.load(Ordering::Acquire);
    if s1 == 0 || s1 & 1 == 1 {
        return None;
    }
    let snap = OpticalSnapshot {
        lux_milli: OPT.lux_milli.load(Ordering::Relaxed),
        proximity_raw: OPT.proximity_raw.load(Ordering::Relaxed) as u16,
    };
    fence(Ordering::Acquire);
    let s2 = OPT.seq.load(Ordering::Relaxed);
    (s1 == s2).then_some(snap)
}

// ── Control plane ────────────────────────────────────────────────────────────

static CTRL_ENV_ENABLED: AtomicBool = AtomicBool::new(false);
static CTRL_ENV_PERIOD_MS: AtomicU32 = AtomicU32::new(0);
static CTRL_OPT_ENABLED: AtomicBool = AtomicBool::new(false);
static CTRL_OPT_PERIOD_MS: AtomicU32 = AtomicU32::new(0);
static CTRL_PAUSED: AtomicBool = AtomicBool::new(false);

/// What the sampler should be doing, re-read fresh on every loop turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlSnapshot {
    pub env_enabled: bool,
    pub env_period_ms: u32,
    pub opt_enabled: bool,
    pub opt_period_ms: u32,
    pub paused: bool,
}

/// JVM-task side: publish per-cluster demand (any registrations? fastest
/// requested period?). Written on register/unregister/reset, not per tick.
pub fn set_cluster_demand(
    env_enabled: bool,
    env_period_ms: u32,
    opt_enabled: bool,
    opt_period_ms: u32,
) {
    CTRL_ENV_PERIOD_MS.store(env_period_ms, Ordering::Release);
    CTRL_OPT_PERIOD_MS.store(opt_period_ms, Ordering::Release);
    CTRL_ENV_ENABLED.store(env_enabled, Ordering::Release);
    CTRL_OPT_ENABLED.store(opt_enabled, Ordering::Release);
}

/// JVM-task side: display-sleep gate for the sampler.
pub fn set_paused(paused: bool) {
    CTRL_PAUSED.store(paused, Ordering::Release);
}

/// Sampler side: current demand.
pub fn control() -> ControlSnapshot {
    ControlSnapshot {
        env_enabled: CTRL_ENV_ENABLED.load(Ordering::Acquire),
        env_period_ms: CTRL_ENV_PERIOD_MS.load(Ordering::Acquire),
        opt_enabled: CTRL_OPT_ENABLED.load(Ordering::Acquire),
        opt_period_ms: CTRL_OPT_PERIOD_MS.load(Ordering::Acquire),
        paused: CTRL_PAUSED.load(Ordering::Acquire),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The cells are process-wide statics; serialize tests that touch them.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset_cells() {
        // Restore the "never published" state between tests. seq must end
        // even; 0 is the reserved never-published value.
        ENV.seq.store(0, Ordering::SeqCst);
        OPT.seq.store(0, Ordering::SeqCst);
        set_cluster_demand(false, 0, false, 0);
        set_paused(false);
    }

    #[test]
    fn read_returns_none_before_first_publish() {
        let _g = TEST_LOCK.lock().unwrap();
        reset_cells();
        assert_eq!(read_env().map(|s| s.press_pa), None);
        assert!(read_optical().is_none());
    }

    #[test]
    fn publish_then_read_roundtrips() {
        let _g = TEST_LOCK.lock().unwrap();
        reset_cells();
        publish_env(&EnvSnapshot {
            temp_centi_c: -573,
            hum_milli_pct: 45_000,
            press_pa: 101_325,
            gas_ohm: 50_000,
        });
        let s = read_env().expect("published value must be readable");
        assert_eq!(s.temp_centi_c, -573);
        assert_eq!(s.hum_milli_pct, 45_000);
        assert_eq!(s.press_pa, 101_325);
        assert_eq!(s.gas_ohm, 50_000);

        publish_optical(&OpticalSnapshot {
            lux_milli: 300_000,
            proximity_raw: 2047,
        });
        let o = read_optical().unwrap();
        assert_eq!(o.lux_milli, 300_000);
        assert_eq!(o.proximity_raw, 2047);
        // Non-consuming: a second read sees the same value.
        assert_eq!(read_optical().unwrap().lux_milli, 300_000);
    }

    #[test]
    fn torn_read_is_skipped_not_spun() {
        let _g = TEST_LOCK.lock().unwrap();
        reset_cells();
        publish_env(&EnvSnapshot::default());
        // Simulate a writer parked mid-publish (odd seq).
        let s = ENV.seq.load(Ordering::Relaxed);
        ENV.seq.store(s + 1, Ordering::Relaxed);
        assert!(read_env().is_none(), "odd seq must read as None");
        ENV.seq.store(s + 2, Ordering::Relaxed);
        assert!(read_env().is_some());
    }

    #[test]
    fn control_roundtrips() {
        let _g = TEST_LOCK.lock().unwrap();
        reset_cells();
        set_cluster_demand(true, 192, false, 0);
        set_paused(true);
        let c = control();
        assert!(c.env_enabled && !c.opt_enabled && c.paused);
        assert_eq!(c.env_period_ms, 192);
    }

    /// Hammer the seqlock from a real writer thread and assert the reader
    /// never observes a mixed (torn) snapshot. The writer publishes
    /// snapshots whose four fields encode the same generation counter, so
    /// any tear is detectable as a field mismatch.
    #[test]
    fn hammer_no_torn_snapshots() {
        let _g = TEST_LOCK.lock().unwrap();
        reset_cells();
        let stop = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let mut gen: u32 = 1;
                while !stop.load(Ordering::Relaxed) {
                    publish_env(&EnvSnapshot {
                        temp_centi_c: gen as i32,
                        hum_milli_pct: gen,
                        press_pa: gen,
                        gas_ohm: gen,
                    });
                    gen = gen.wrapping_add(1);
                }
            });
            let mut seen = 0u32;
            while seen < 100_000 {
                if let Some(s) = read_env() {
                    let g = s.hum_milli_pct;
                    assert_eq!(s.temp_centi_c, g as i32, "torn snapshot");
                    assert_eq!(s.press_pa, g, "torn snapshot");
                    assert_eq!(s.gas_ohm, g, "torn snapshot");
                    seen += 1;
                }
            }
            stop.store(true, Ordering::Relaxed);
        });
    }
}
