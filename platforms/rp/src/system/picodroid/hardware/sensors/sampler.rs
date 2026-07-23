// SPDX-License-Identifier: GPL-3.0-only
//! Sensor sampler — owns ALL sensor I²C, off the JVM/UI task.
//!
//! Device: a dedicated FreeRTOS task ("sensor", BG tier, core 0) spawned
//! pre-scheduler from `boot.rs::start_tasks`. Sim: a lazily spawned
//! `std::thread` publishing the synthetic triangle waves. Sensor-less
//! device builds get a no-op facade — no task, no statics; the drain's
//! default shims keep zeros-at-cadence behavior.
//!
//! The sampler exchanges only plain scalars with the JVM task through
//! [`super::mailbox`]: readings out via the seqlock cells, demand in via
//! the control atomics. It never touches the JVM heap.
//!
//! Cadence is demand-driven (Android-style): per cluster, the fastest
//! period any registration asked for, floored by conversion physics
//! (BME688 forced TPHG ≈ 45 ms). No registrations, or display sleep →
//! the task parks on its wake semaphore: zero I²C, zero wakeups.

// ── Facade ───────────────────────────────────────────────────────────────────

/// Wake the sampler to re-read control state (register/unregister/reset/
/// pause/resume). Level-triggered: kicks coalesce.
#[cfg(all(not(feature = "sim"), any_sensor))]
pub fn kick() {
    device::kick();
}
#[cfg(feature = "sim")]
pub fn kick() {
    sim_backing::kick();
}
#[cfg(all(not(feature = "sim"), not(any_sensor)))]
pub fn kick() {}

/// Quiesce sampling for display sleep (called beside `tick_source::pause`).
#[allow(dead_code)] // only called on has_buttons boards with an idle timeout
pub fn pause() {
    #[cfg(any(any_sensor, feature = "sim"))]
    super::mailbox::set_paused(true);
    kick();
}

/// Resume sampling after display wake (called beside `tick_source::resume`).
#[allow(dead_code)] // only called on has_buttons boards with an idle timeout
pub fn resume() {
    #[cfg(any(any_sensor, feature = "sim"))]
    super::mailbox::set_paused(false);
    kick();
}

#[cfg(all(not(feature = "sim"), any_sensor))]
pub use device::spawn;

// ── Device backing: FreeRTOS task ────────────────────────────────────────────

#[cfg(all(not(feature = "sim"), any_sensor))]
mod device {
    use core::cell::UnsafeCell;

    use freertos_rust::{Duration, Semaphore, Task, TaskPriority};

    use super::super::mailbox;
    use super::super::{EnvSnapshot, OpticalSnapshot};

    mod sensor_table {
        include!(concat!(env!("OUT_DIR"), "/sensor_table.rs"));
    }

    /// BME688 forced-mode TPHG conversion time (1x oversampling + gas
    /// heater), used both as the post-trigger wait and as the physics
    /// floor on the effective sampling period.
    #[cfg(sensor_bme688)]
    const BME_CONV_MS: u64 = 45;
    /// Re-check interval and cap when a conversion runs late.
    #[cfg(sensor_bme688)]
    const BME_LATE_RECHECK_MS: u64 = 5;
    #[cfg(sensor_bme688)]
    const BME_LATE_RETRIES: u8 = 5;
    /// Idle ceiling on the pacing wait — a lost edge case can only ever
    /// stall sampling by this much.
    const MAX_WAIT_MS: u64 = 1000;

    struct SemCell(UnsafeCell<Option<Semaphore>>);
    // SAFETY: written once pre-scheduler; only shared references afterwards.
    unsafe impl Sync for SemCell {}
    static WAKE: SemCell = SemCell(UnsafeCell::new(None));

    fn wake_sem() -> Option<&'static Semaphore> {
        unsafe { (*WAKE.0.get()).as_ref() }
    }

    /// Wake the task out of its pacing/parked wait. Callable from any task;
    /// the binary semaphore latches a kick delivered before the first
    /// `take`, so a registration that lands before the sampler ever ran is
    /// not lost.
    pub fn kick() {
        if let Some(s) = wake_sem() {
            let _ = s.give();
        }
    }

    /// Create the wake semaphore, init the sensor I²C buses, and spawn the
    /// task. Must run pre-scheduler (single-threaded): `i2c::init`'s
    /// check-then-act is not task-safe, and after this point the sampler
    /// and Java `I2cDevice` users may share a bus (per-transfer bus locks
    /// make that safe once init has happened).
    pub fn spawn() {
        // SAFETY: pre-scheduler, single-threaded.
        unsafe {
            *WAKE.0.get() = Some(Semaphore::new_binary().expect("sensor wake sem"));
        }
        for cfg in sensor_table::SENSORS {
            crate::hal::i2c::init(cfg.bus_id);
        }
        Task::new()
            .name("sensor")
            .stack_size(crate::boot_budget::SENSOR_STACK_WORDS)
            .priority(TaskPriority(crate::task_priority::PRIORITY_SENSOR))
            .core_affinity(0b01) // core 0 with the JVM: no cross-core visibility hazard
            .start(|_| run())
            .expect("sensor task");
    }

    fn now_ms() -> u64 {
        crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
    }

    fn run() {
        let mut bme = BmeCluster::new();
        let mut ltr = LtrCluster::new();
        defmt::info!("sensor: task up");
        loop {
            let c = mailbox::control();
            if c.paused || (!c.env_enabled && !c.opt_enabled) {
                // Abandon any in-flight conversion; re-trigger on resume
                // (one wasted conversion, no stuck state).
                bme.abandon();
                if let Some(s) = wake_sem() {
                    let _ = s.take(Duration::infinite());
                }
                continue;
            }

            let now = now_ms();
            let mut next_deadline = now + MAX_WAIT_MS;
            if c.env_enabled {
                if let Some(d) = bme.service(now, c.env_period_ms as u64) {
                    next_deadline = next_deadline.min(d);
                }
            }
            if c.opt_enabled {
                if let Some(d) = ltr.service(now, c.opt_period_ms as u64) {
                    next_deadline = next_deadline.min(d);
                }
            }

            // The pacing wait doubles as the control-interrupt point: a
            // kick() (register/unregister/pause) ends it early and the
            // loop re-reads control.
            let delay = next_deadline.saturating_sub(now_ms()).clamp(1, MAX_WAIT_MS);
            if let Some(s) = wake_sem() {
                let _ = s.take(Duration::ms(delay as u32));
            }
        }
    }

    // ── BME688 cluster ──────────────────────────────────────────────────────

    struct HalI2c {
        bus_id: u8,
    }
    #[cfg(sensor_bme688)]
    impl crate::drivers::bme688::I2cBus for HalI2c {
        fn write(&mut self, addr: u8, data: &[u8]) -> i32 {
            crate::hal::i2c::write_slice(self.bus_id, addr, data)
        }
        fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32 {
            crate::hal::i2c::read_slice(self.bus_id, addr, buf)
        }
    }
    #[cfg(sensor_ltr559)]
    impl crate::drivers::ltr559::I2cBus for HalI2c {
        fn write(&mut self, addr: u8, data: &[u8]) -> i32 {
            crate::hal::i2c::write_slice(self.bus_id, addr, data)
        }
        fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32 {
            crate::hal::i2c::read_slice(self.bus_id, addr, buf)
        }
    }

    #[cfg(sensor_bme688)]
    enum BmeState {
        Idle,
        Converting { ready_at: u64, retries: u8 },
    }

    #[cfg(sensor_bme688)]
    struct BmeCluster {
        driver: Option<crate::drivers::bme688::Bme688<HalI2c>>,
        probed: bool,
        state: BmeState,
        next_due: u64,
        err_streak: u32,
    }

    #[cfg(sensor_bme688)]
    impl BmeCluster {
        fn new() -> Self {
            Self {
                driver: None,
                probed: false,
                state: BmeState::Idle,
                next_due: 0,
                err_streak: 0,
            }
        }

        fn abandon(&mut self) {
            self.state = BmeState::Idle;
        }

        /// Advance the trigger→wait→read state machine. Returns the next
        /// deadline this cluster needs service at (None if chip absent).
        fn service(&mut self, now: u64, period_ms: u64) -> Option<u64> {
            if !self.probed {
                self.probed = true;
                self.driver = Self::probe();
                if self.driver.is_none() {
                    // No chip on the bus: publish one valid default so the
                    // drain delivers zeros at cadence (pre-move behavior).
                    defmt::warn!("sensor: bme688 probe failed — publishing zeros");
                    mailbox::publish_env(&EnvSnapshot::default());
                }
            }
            let driver = self.driver.as_mut()?;

            match self.state {
                BmeState::Idle => {
                    if now >= self.next_due {
                        let _ = driver.trigger_forced();
                        // Period is trigger-to-trigger; the conversion time
                        // is the physics floor an app-requested FASTEST
                        // cannot beat.
                        self.next_due = now + period_ms.max(BME_CONV_MS);
                        self.state = BmeState::Converting {
                            ready_at: now + BME_CONV_MS,
                            retries: 0,
                        };
                    }
                }
                BmeState::Converting { ready_at, retries } => {
                    if now >= ready_at {
                        if driver.poll_ready(1) {
                            match driver.read_compensated() {
                                Ok(r) => {
                                    defmt::debug!(
                                        "bme: temp={} hum={} press={} gas={}",
                                        r.temp_centi_c,
                                        r.hum_milli_pct,
                                        r.press_pa,
                                        r.gas_ohm
                                    );
                                    mailbox::publish_env(&EnvSnapshot {
                                        temp_centi_c: r.temp_centi_c,
                                        hum_milli_pct: r.hum_milli_pct,
                                        press_pa: r.press_pa,
                                        gas_ohm: r.gas_ohm,
                                    });
                                    self.err_streak = 0;
                                }
                                Err(_) => self.note_err("read"),
                            }
                            self.state = BmeState::Idle;
                        } else if retries < BME_LATE_RETRIES {
                            self.state = BmeState::Converting {
                                ready_at: now + BME_LATE_RECHECK_MS,
                                retries: retries + 1,
                            };
                        } else {
                            // Conversion never came ready — skip it; the
                            // mailbox keeps the last good reading.
                            self.note_err("ready");
                            self.state = BmeState::Idle;
                        }
                    }
                }
            }

            let state_deadline = match self.state {
                BmeState::Idle => self.next_due,
                BmeState::Converting { ready_at, .. } => ready_at,
            };
            Some(state_deadline)
        }

        fn probe() -> Option<crate::drivers::bme688::Bme688<HalI2c>> {
            for cfg in sensor_table::SENSORS {
                if matches!(cfg.kind, sensor_table::SensorKind::Bme688) {
                    let bus = HalI2c { bus_id: cfg.bus_id };
                    return crate::drivers::bme688::Bme688::new(bus, cfg.addr).ok();
                }
            }
            None
        }

        fn note_err(&mut self, what: &str) {
            self.err_streak += 1;
            // Rate-limit: the first error of a streak, then every 16th.
            if self.err_streak % 16 == 1 {
                defmt::warn!(
                    "sensor: bme688 {=str} error (streak {})",
                    what,
                    self.err_streak
                );
            }
        }
    }

    /// Board has env registrations but no BME688 configured: publish one
    /// default snapshot so the drain delivers zeros at cadence.
    #[cfg(not(sensor_bme688))]
    struct BmeCluster {
        published: bool,
    }
    #[cfg(not(sensor_bme688))]
    impl BmeCluster {
        fn new() -> Self {
            Self { published: false }
        }
        fn abandon(&mut self) {}
        fn service(&mut self, _now: u64, _period_ms: u64) -> Option<u64> {
            if !self.published {
                self.published = true;
                mailbox::publish_env(&EnvSnapshot::default());
            }
            None
        }
    }

    // ── LTR559 cluster ──────────────────────────────────────────────────────

    #[cfg(sensor_ltr559)]
    struct LtrCluster {
        driver: Option<crate::drivers::ltr559::Ltr559<HalI2c>>,
        probed: bool,
        next_due: u64,
        err_streak: u32,
    }

    #[cfg(sensor_ltr559)]
    impl LtrCluster {
        fn new() -> Self {
            Self {
                driver: None,
                probed: false,
                next_due: 0,
                err_streak: 0,
            }
        }

        /// Measure at the demanded cadence (no conversion latency to pace).
        /// Runs freely during BME conversions — those happen on-chip, so
        /// the bus is idle and LTR staleness stays bounded by its own
        /// period rather than the BME conversion time.
        fn service(&mut self, now: u64, period_ms: u64) -> Option<u64> {
            if !self.probed {
                self.probed = true;
                self.driver = Self::probe();
                if self.driver.is_none() {
                    defmt::warn!("sensor: ltr559 probe failed — publishing zeros");
                    mailbox::publish_optical(&OpticalSnapshot::default());
                }
            }
            let driver = self.driver.as_mut()?;

            if now >= self.next_due {
                match driver.measure() {
                    Ok(r) => {
                        mailbox::publish_optical(&OpticalSnapshot {
                            lux_milli: r.lux_milli,
                            proximity_raw: r.proximity_raw,
                        });
                        self.err_streak = 0;
                    }
                    Err(_) => {
                        self.err_streak += 1;
                        if self.err_streak % 16 == 1 {
                            defmt::warn!("sensor: ltr559 read error (streak {})", self.err_streak);
                        }
                    }
                }
                self.next_due = now + period_ms.max(1);
            }
            Some(self.next_due)
        }

        fn probe() -> Option<crate::drivers::ltr559::Ltr559<HalI2c>> {
            for cfg in sensor_table::SENSORS {
                if matches!(cfg.kind, sensor_table::SensorKind::Ltr559) {
                    let bus = HalI2c { bus_id: cfg.bus_id };
                    return crate::drivers::ltr559::Ltr559::new(bus, cfg.addr).ok();
                }
            }
            None
        }
    }

    #[cfg(not(sensor_ltr559))]
    struct LtrCluster {
        published: bool,
    }
    #[cfg(not(sensor_ltr559))]
    impl LtrCluster {
        fn new() -> Self {
            Self { published: false }
        }
        fn service(&mut self, _now: u64, _period_ms: u64) -> Option<u64> {
            if !self.published {
                self.published = true;
                mailbox::publish_optical(&OpticalSnapshot::default());
            }
            None
        }
    }
}

// ── Sim backing: std::thread ─────────────────────────────────────────────────

#[cfg(feature = "sim")]
mod sim_backing {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    use super::super::mailbox;
    use super::super::{EnvSnapshot, OpticalSnapshot};

    static SPAWNED: AtomicBool = AtomicBool::new(false);
    /// Kicked flag under the mutex — prevents the lost-wakeup race between
    /// the sampler reading control and parking on the condvar.
    static KICKED: Mutex<bool> = Mutex::new(false);
    static WAKE: Condvar = Condvar::new();

    pub fn kick() {
        if !SPAWNED.swap(true, Ordering::SeqCst) {
            // Host thread machinery has no device analog (the device task's
            // stack enters via the boot budget); keep pthread internals off
            // the simulated heap (docs/parity-audit.md M1 routing), like
            // the lvgl-tick thread does.
            let _spawn_bypass = crate::sim_allocator::bypass();
            std::thread::Builder::new()
                .name("sensor-sim".into())
                .spawn(|| {
                    let _bypass = crate::sim_allocator::bypass();
                    run();
                })
                .expect("spawn sensor-sim thread");
        }
        *KICKED.lock().expect("sensor-sim KICKED mutex poisoned") = true;
        WAKE.notify_all();
    }

    /// Park until kicked (None) or for at most `timeout` (Some).
    fn wait(timeout: Option<Duration>) {
        let mut kicked = KICKED.lock().expect("sensor-sim KICKED mutex poisoned");
        let deadline = timeout.map(|t| Instant::now() + t);
        while !*kicked {
            match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        break;
                    }
                    let (g, _) = WAKE
                        .wait_timeout(kicked, d - now)
                        .expect("sensor-sim condvar poisoned");
                    kicked = g;
                }
                None => {
                    kicked = WAKE.wait(kicked).expect("sensor-sim condvar poisoned");
                }
            }
        }
        *kicked = false;
    }

    fn run() {
        let start = Instant::now();
        let mut next_env_ms: u64 = 0;
        let mut next_opt_ms: u64 = 0;
        loop {
            let c = mailbox::control();
            if c.paused || (!c.env_enabled && !c.opt_enabled) {
                wait(None);
                continue;
            }

            let now = start.elapsed().as_millis() as u64;
            let mut next_deadline = now + 1000;
            if c.env_enabled {
                if now >= next_env_ms {
                    mailbox::publish_env(&synth_env(now));
                    next_env_ms = now + (c.env_period_ms as u64).max(1);
                }
                next_deadline = next_deadline.min(next_env_ms);
            }
            if c.opt_enabled {
                if now >= next_opt_ms {
                    mailbox::publish_optical(&synth_optical(now));
                    next_opt_ms = now + (c.opt_period_ms as u64).max(1);
                }
                next_deadline = next_deadline.min(next_opt_ms);
            }

            let dt = next_deadline.saturating_sub(start.elapsed().as_millis() as u64);
            wait(Some(Duration::from_millis(dt.max(1))));
        }
    }

    /// Synthetic BME688 values: a slow triangle wave around realistic
    /// indoor values (~3.2 s period, matching the pre-task sim behavior of
    /// 200 × 16 ms ticks). The wobble proves the UI re-renders on sensor
    /// data and keeps threshold-breach animations exercisable; amplitudes
    /// stay inside the default threshold band so steady state is calm.
    /// The sim I²C fake is bypassed on purpose — its calibration registers
    /// are all zero, so the real driver would compensate everything to 0.
    fn synth_env(elapsed_ms: u64) -> EnvSnapshot {
        let phase = ((elapsed_ms % 3200) as f32) / 1600.0 - 1.0; // triangle in [-1, 1)
        EnvSnapshot {
            temp_centi_c: (2200.0 + phase * 50.0) as i32, // 22.0 ± 0.5 °C
            hum_milli_pct: (45_000.0 + phase * 2_000.0) as u32, // 45 ± 2 %
            press_pa: (101_325.0 + phase * 100.0) as u32, // 1013.25 ± 1 hPa
            gas_ohm: (50_000.0 + phase * 5_000.0) as u32, // 50 kΩ ± 5 kΩ
        }
    }

    /// Synthetic LTR559: 300 ± 50 lx triangle (no LTR fake exists in the
    /// sim I²C responder; without this the sim would read 0 lx forever).
    fn synth_optical(elapsed_ms: u64) -> OpticalSnapshot {
        let phase = ((elapsed_ms % 3200) as f32) / 1600.0 - 1.0;
        OpticalSnapshot {
            lux_milli: (300_000.0 + phase * 50_000.0) as u32,
            proximity_raw: 0,
        }
    }
}
