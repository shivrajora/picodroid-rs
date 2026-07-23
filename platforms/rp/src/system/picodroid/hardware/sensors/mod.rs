// SPDX-License-Identifier: GPL-3.0-only
//! SensorManager native method implementations.
//!
//! This module owns the JVM-facing side only: registrations, GC roots, and
//! event delivery on the JVM main task. All sensor I²C runs on a dedicated
//! sampler (FreeRTOS task on device, `std::thread` on sim — see
//! [`sampler`]), which exchanges plain scalars with this module through the
//! lock-free [`mailbox`]: readings in via seqlock cells the drain reads at
//! each registration's due tick, demand out via control atomics published
//! on register/unregister. No JVM heap reference ever crosses the task
//! boundary.

use pico_jvm::{
    array_heap::ArrayHeap,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

#[cfg(any(any_sensor, feature = "sim", test))]
pub mod mailbox;
pub mod sampler;

/// Latest BME688 cluster reading, as plain scalars (see `deliver_event`'s
/// scaling for the Java-facing units).
#[derive(Clone, Copy, Default)]
pub struct EnvSnapshot {
    pub temp_centi_c: i32,
    pub hum_milli_pct: u32,
    pub press_pa: u32,
    pub gas_ohm: u32,
}

/// Latest LTR559 cluster reading.
#[derive(Clone, Copy, Default)]
pub struct OpticalSnapshot {
    pub lux_milli: u32,
    pub proximity_raw: u16,
}

// ── Sensor type IDs (must match picodroid.hardware.Sensor.java) ─────────────

const TYPE_LIGHT: i32 = 5;
const TYPE_PRESSURE: i32 = 6;
const TYPE_PROXIMITY: i32 = 8;
const TYPE_RELATIVE_HUMIDITY: i32 = 12;
const TYPE_AMBIENT_TEMPERATURE: i32 = 13;
const TYPE_GAS_RESISTANCE: i32 = 0x10001;

// ── Sensor Java field indices ───────────────────────────────────────────────

mod fields {
    pub const TYPE: usize = 0;
    pub const NAME: usize = 1;
    pub const VENDOR: usize = 2;
    pub const MAX_RANGE: usize = 3;
    pub const RESOLUTION: usize = 4;
    pub const MIN_DELAY: usize = 5;
}

mod event_fields {
    pub const SENSOR: usize = 0;
    pub const VALUES: usize = 1;
    pub const ACCURACY: usize = 2;
    pub const TIMESTAMP: usize = 3;
}

// ── Static state ────────────────────────────────────────────────────────────

const MAX_REGISTRATIONS: usize = 8;

struct Registration {
    listener_obj: u16,
    sensor_type: i32,
    sensor_obj: u16,
    /// Recycled SensorEvent delivered to this registration on every tick.
    /// Allocated once at registerListener; deliver_event only rewrites
    /// `values[0]` and `timestamp`, so steady-state delivery allocates
    /// nothing. Matches Android, which documents that SensorManager may
    /// reuse SensorEvent instances (listeners must copy values out).
    event_obj: u16,
    /// The float[1] backing `event_obj.values`.
    values_arr: u16,
    period_ticks: u32,
    ticks_until_due: u32,
}

struct SensorState {
    registrations: [Option<Registration>; MAX_REGISTRATIONS],
    sensor_objs: [Option<u16>; 6], // temp, hum, press, gas, light, proximity
    /// Live registration count — the drain's sensors-off fast path is a
    /// single load+branch on this instead of scanning the slots. Invariant:
    /// `active_regs == 0` implies `pending` is all-None (unregister and the
    /// delivery pacer both clear pending for dead slots).
    active_regs: u8,
    /// Values snapshotted at each registration's due tick, parked until the
    /// delivery pacer gets to them. Latest-wins: a new due tick overwrites an
    /// undelivered value (Android's documented drop-under-backpressure).
    pending: [Option<f32>; MAX_REGISTRATIONS],
    /// Round-robin start slot for the delivery pacer, so no slot starves.
    deliver_cursor: usize,
}

impl SensorState {
    const fn new() -> Self {
        Self {
            registrations: [const { None }; MAX_REGISTRATIONS],
            sensor_objs: [None; 6],
            active_regs: 0,
            pending: [None; MAX_REGISTRATIONS],
            deliver_cursor: 0,
        }
    }
}

static mut STATE: SensorState = SensorState::new();

fn state() -> &'static mut SensorState {
    unsafe { &mut *core::ptr::addr_of_mut!(STATE) }
}

fn delay_to_ticks(delay: i32) -> u32 {
    match delay {
        0 => 1,  // FASTEST: every tick
        1 => 2,  // GAME: ~32 ms
        2 => 4,  // UI: ~64 ms
        _ => 12, // NORMAL: ~192 ms
    }
}

fn sensor_type_index(sensor_type: i32) -> Option<usize> {
    match sensor_type {
        TYPE_AMBIENT_TEMPERATURE => Some(0),
        TYPE_RELATIVE_HUMIDITY => Some(1),
        TYPE_PRESSURE => Some(2),
        TYPE_GAS_RESISTANCE => Some(3),
        TYPE_LIGHT => Some(4),
        TYPE_PROXIMITY => Some(5),
        _ => None,
    }
}

// ── Native method implementations ───────────────────────────────────────────

/// Emit GC roots for every object ref stored in sensor native state.
///
/// Called by `PicodroidNativeHandler::gc_visit_roots`. Without this, the
/// listener (Activity) and Sensor object indices kept in `registrations`
/// and `sensor_objs` are invisible to the mark phase; a GC between
/// `onSensorChanged` callbacks would sweep them, and the next sensor
/// tick would dispatch into a freed slot (NoSuchMethod / InvalidReference).
pub fn visit_gc_roots(visit: &mut dyn FnMut(Value)) {
    let st = state();
    for reg in st.registrations.iter().flatten() {
        visit(Value::ObjectRef(reg.listener_obj));
        visit(Value::ObjectRef(reg.sensor_obj));
        visit(Value::ObjectRef(reg.event_obj));
        visit(Value::ArrayRef(reg.values_arr));
    }
    for obj in st.sensor_objs.iter().flatten() {
        visit(Value::ObjectRef(*obj));
    }
}

pub fn get_default_sensor(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut pico_jvm::heap::StringTable,
) -> Result<Option<Value>, JvmError> {
    // args[0] = this (SensorManager), args[1] = type (int)
    let sensor_type = match args.get(1) {
        Some(Value::Int(t)) => *t,
        _ => return Err(JvmError::InvalidBytecode),
    };

    let idx = match sensor_type_index(sensor_type) {
        Some(i) => i,
        None => return Ok(Some(Value::Null)),
    };

    let st = state();
    if let Some(obj) = st.sensor_objs[idx] {
        return Ok(Some(Value::ObjectRef(obj)));
    }

    let obj = objects
        .alloc(crate::shrink_names::shrink_class(
            "picodroid/hardware/Sensor",
        ))
        .ok_or(JvmError::StackOverflow)?;

    objects
        .set_field(obj, fields::TYPE, Value::Int(sensor_type))
        .ok_or(JvmError::StackOverflow)?;

    let (name, vendor, max_range, resolution, min_delay): (
        &'static [u8],
        &'static [u8],
        f32,
        f32,
        i32,
    ) = match sensor_type {
        TYPE_AMBIENT_TEMPERATURE => (b"BME688 Temperature", b"Bosch", 85.0, 0.01, 200_000),
        TYPE_RELATIVE_HUMIDITY => (b"BME688 Humidity", b"Bosch", 100.0, 0.008, 200_000),
        TYPE_PRESSURE => (b"BME688 Pressure", b"Bosch", 1100.0, 0.18, 200_000),
        TYPE_GAS_RESISTANCE => (b"BME688 Gas", b"Bosch", 500000.0, 1.0, 200_000),
        TYPE_LIGHT => (b"LTR559 Light", b"Lite-On", 64000.0, 0.01, 100_000),
        TYPE_PROXIMITY => (b"LTR559 Proximity", b"Lite-On", 2047.0, 1.0, 100_000),
        _ => return Ok(Some(Value::Null)),
    };

    let name_ref = strings.intern(name).ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj, fields::NAME, Value::Reference(name_ref))
        .ok_or(JvmError::StackOverflow)?;
    let vendor_ref = strings.intern(vendor).ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj, fields::VENDOR, Value::Reference(vendor_ref))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj, fields::MAX_RANGE, Value::Float(max_range))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj, fields::RESOLUTION, Value::Float(resolution))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj, fields::MIN_DELAY, Value::Int(min_delay))
        .ok_or(JvmError::StackOverflow)?;

    st.sensor_objs[idx] = Some(obj);
    Ok(Some(Value::ObjectRef(obj)))
}

pub fn register_listener(
    args: &[Value],
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    // args[0] = this (SensorManager)
    // args[1] = SensorEventListener
    // args[2] = Sensor
    // args[3] = samplingPeriodUs (int delay constant)
    let listener_obj = match args.get(1) {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Ok(Some(Value::Int(0))), // false
    };
    let sensor_obj = match args.get(2) {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Ok(Some(Value::Int(0))),
    };
    let delay = match args.get(3) {
        Some(Value::Int(d)) => *d,
        _ => 3, // default NORMAL
    };

    let sensor_type = match objects.get_field(sensor_obj, fields::TYPE) {
        Some(Value::Int(t)) => t,
        _ => return Ok(Some(Value::Int(0))),
    };

    let st = state();
    let period_ticks = delay_to_ticks(delay);

    // Check for existing registration with same listener + sensor type
    for reg in st.registrations.iter_mut().flatten() {
        if reg.listener_obj == listener_obj && reg.sensor_type == sensor_type {
            reg.period_ticks = period_ticks;
            reg.ticks_until_due = 0;
            publish_control(st);
            return Ok(Some(Value::Int(1))); // true
        }
    }

    // Find empty slot
    for i in 0..MAX_REGISTRATIONS {
        if st.registrations[i].is_none() {
            // Allocate this registration's recycled SensorEvent up front,
            // with its full field span so no set_field ever lazy-grows the
            // fields arena. The immutable payload (sensor, values ref,
            // accuracy) is written once here; deliver_event only rewrites
            // values[0] and timestamp, so per-event delivery is allocation-
            // free. Rooted via visit_gc_roots until unregisterListener.
            let event_obj = objects
                .alloc_with_field_count(
                    crate::shrink_names::shrink_class("picodroid/hardware/SensorEvent"),
                    event_fields::TIMESTAMP + 1,
                )
                .ok_or(JvmError::StackOverflow)?;
            let values_arr = arrays
                .alloc(pico_jvm::array_heap::ATYPE_FLOAT, 1)
                .ok_or(JvmError::StackOverflow)?;
            #[cfg(feature = "mem-diag")]
            crate::system::mem_diag::note_native_alloc(2); // event_obj + values_arr
            objects
                .set_field(
                    event_obj,
                    event_fields::SENSOR,
                    Value::ObjectRef(sensor_obj),
                )
                .ok_or(JvmError::StackOverflow)?;
            objects
                .set_field(event_obj, event_fields::VALUES, Value::ArrayRef(values_arr))
                .ok_or(JvmError::StackOverflow)?;
            objects
                .set_field(event_obj, event_fields::ACCURACY, Value::Int(3))
                .ok_or(JvmError::StackOverflow)?;

            st.registrations[i] = Some(Registration {
                listener_obj,
                sensor_type,
                sensor_obj,
                event_obj,
                values_arr,
                period_ticks,
                ticks_until_due: 0,
            });
            st.active_regs += 1;
            publish_control(st);
            return Ok(Some(Value::Int(1))); // true
        }
    }

    Ok(Some(Value::Int(0))) // false — no free slot
}

pub fn unregister_listener(args: &[Value]) -> Result<Option<Value>, JvmError> {
    // args[0] = this (SensorManager), args[1] = listener
    let listener_obj = match args.get(1) {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Ok(None),
    };

    let st = state();
    for (i, slot) in st.registrations.iter_mut().enumerate() {
        if let Some(reg) = slot {
            if reg.listener_obj == listener_obj {
                *slot = None;
                st.pending[i] = None;
                st.active_regs = st.active_regs.saturating_sub(1);
            }
        }
    }
    publish_control(st);
    Ok(None)
}

/// Clear all sensor native state across app restarts (pdb reinstall):
/// stale `u16` refs from the previous app must not survive into a reset
/// heap (they'd be fed to `visit_gc_roots` and delivery). Also publishes
/// all-disabled demand so the sampler parks between apps.
pub fn reset() {
    let st = state();
    st.registrations = [const { None }; MAX_REGISTRATIONS];
    st.sensor_objs = [None; 6];
    st.active_regs = 0;
    st.pending = [None; MAX_REGISTRATIONS];
    st.deliver_cursor = 0;
    publish_control(st);
}

/// Push per-cluster demand (enabled + fastest requested period) to the
/// sampler through the mailbox control plane, then kick it awake to
/// re-read. Called on register/unregister/reset — never per tick.
#[cfg(any(any_sensor, feature = "sim"))]
fn publish_control(st: &SensorState) {
    const TICK_MS: u32 = 16;
    let mut env_min: Option<u32> = None;
    let mut opt_min: Option<u32> = None;
    for reg in st.registrations.iter().flatten() {
        let period_ms = reg.period_ticks.max(1) * TICK_MS;
        match sensor_type_index(reg.sensor_type) {
            Some(idx) if idx < 4 => env_min = Some(env_min.map_or(period_ms, |m| m.min(period_ms))),
            Some(_) => opt_min = Some(opt_min.map_or(period_ms, |m| m.min(period_ms))),
            None => {}
        }
    }
    mailbox::set_cluster_demand(
        env_min.is_some(),
        env_min.unwrap_or(0),
        opt_min.is_some(),
        opt_min.unwrap_or(0),
    );
    sampler::kick();
}

/// Sensor-less device builds: no sampler exists; the drain serves zeros
/// from default snapshots at cadence.
#[cfg(all(not(any_sensor), not(feature = "sim")))]
fn publish_control(_st: &SensorState) {}

// ── Main-loop event drain ───────────────────────────────────────────────────

/// Max Java `onSensorChanged` deliveries per tick. One interpreted callback
/// measured 5–8 ms on RP2350 — and an app doing real work inside one
/// (picoenvmon's 1 Hz smoothed fan-out) ~37 ms — so delivering all five due
/// registrations in a single tick was the bulk of a ~100 ms UI-tick stall.
/// Due events park in `SensorState::pending` and drain one per 16 ms tick;
/// at the NORMAL 192 ms period that leaves 12 ticks of headroom per cycle.
const MAX_DELIVERIES_PER_TICK: u32 = 1;

/// Poll sensors and deliver SensorEvent callbacks for any due registrations.
/// Called once per main-loop tick (~16 ms).
///
/// Values are snapshotted into `pending` at each registration's due tick,
/// then the delivery pacer below feeds them to Java at most
/// [`MAX_DELIVERIES_PER_TICK`] per tick so a burst of due registrations
/// can't stall the single-threaded UI tick.
#[cfg(not(test))]
pub fn drain_sensor_events(
    jvm: &mut pico_jvm::Jvm,
    heap: &mut pico_jvm::SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    let st = state();

    // Sensors-off fast path: one load + one branch per tick. active_regs==0
    // implies pending is all-None (see SensorState invariant).
    if st.active_regs == 0 {
        return;
    }

    // Mark due registrations (per slot: each registration keeps its own
    // cadence; a faster registration of the same type no longer drags
    // slower ones along).
    let mut due = [false; MAX_REGISTRATIONS];
    let mut bme_needed = false;
    let mut ltr_needed = false;
    for (i, slot) in st.registrations.iter_mut().enumerate() {
        let Some(reg) = slot else { continue };
        if reg.ticks_until_due == 0 {
            due[i] = true;
            match sensor_type_index(reg.sensor_type) {
                Some(idx) if idx < 4 => bme_needed = true,
                Some(_) => ltr_needed = true,
                None => {}
            }
            reg.ticks_until_due = reg.period_ticks;
        }
        reg.ticks_until_due = reg.ticks_until_due.saturating_sub(1);
    }

    if !due.iter().any(|&d| d) && !st.pending.iter().any(|p| p.is_some()) {
        return;
    }

    // Read the latest sampler-published snapshots only if a due
    // registration needs them this tick. `None` means the sampler hasn't
    // published yet (first conversion still in flight) or the single
    // seqlock read raced a publish — either way, defer and retry next tick.
    let env = if bme_needed {
        read_env_snapshot()
    } else {
        None
    };
    let optical = if ltr_needed {
        read_optical_snapshot()
    } else {
        None
    };

    // Snapshot values for due registrations into the pending table.
    for (i, _) in due.iter().enumerate().filter(|(_, &d)| d) {
        let Some(reg) = &mut st.registrations[i] else {
            continue;
        };
        let value = match sensor_type_index(reg.sensor_type) {
            Some(idx @ 0..=3) => match &env {
                Some(env) => match idx {
                    0 => env.temp_centi_c as f32 / 100.0,
                    1 => env.hum_milli_pct as f32 / 1000.0,
                    2 => env.press_pa as f32 / 100.0, // hPa
                    _ => env.gas_ohm as f32,
                },
                // First conversion still in flight — retry next tick rather
                // than deliver zeros (a 0 °C / 0 % reading would trip the
                // app's threshold alerts at startup).
                None => {
                    reg.ticks_until_due = 0;
                    continue;
                }
            },
            Some(idx @ (4 | 5)) => match &optical {
                Some(o) => {
                    if idx == 4 {
                        o.lux_milli as f32 / 1000.0
                    } else {
                        o.proximity_raw as f32
                    }
                }
                // Sampler hasn't published optical data yet — same defer
                // as the env arm (no fabricated first value).
                None => {
                    reg.ticks_until_due = 0;
                    continue;
                }
            },
            _ => continue,
        };
        st.pending[i] = Some(value);
    }

    // Delivery pacer: feed parked events to Java, round-robin from
    // deliver_cursor, at most MAX_DELIVERIES_PER_TICK per tick.
    let mut budget = MAX_DELIVERIES_PER_TICK;
    for step in 0..MAX_REGISTRATIONS {
        if budget == 0 {
            break;
        }
        let i = (st.deliver_cursor + step) % MAX_REGISTRATIONS;
        let Some(value) = st.pending[i] else { continue };
        // Re-check the slot each iteration: a callback delivered below may
        // have called unregisterListener on a later slot.
        let Some(reg) = &st.registrations[i] else {
            st.pending[i] = None;
            continue;
        };
        st.pending[i] = None;
        budget -= 1;
        st.deliver_cursor = i + 1;

        let listener_obj = reg.listener_obj;
        let event_obj = reg.event_obj;
        let values_arr = reg.values_arr;

        let mut result = deliver_event(
            jvm,
            heap,
            handler,
            listener_obj,
            event_obj,
            values_arr,
            value,
        );
        if matches!(result, Err(JvmError::StackOverflow)) {
            // StackOverflow is the JVM's allocation-failure signal (here it
            // can only come from inside the onSensorChanged call — delivery
            // itself no longer allocates). This drain runs between bytecode
            // executions, so no interpreter safepoint can run the emergency
            // GC a failing opcode would get — collect with native-only roots
            // and redeliver once.
            heap.collect_now(handler);
            result = deliver_event(
                jvm,
                heap,
                handler,
                listener_obj,
                event_obj,
                values_arr,
                value,
            );
        }
        if let Err(e) = result {
            #[cfg(not(feature = "sim"))]
            defmt::warn!("sensors: deliver_event err");
            #[cfg(feature = "sim")]
            eprintln!("[sim] sensor event delivery error: {e}");
            let _ = e;
        }
    }
}

#[cfg(not(test))]
fn deliver_event(
    jvm: &mut pico_jvm::Jvm,
    heap: &mut pico_jvm::SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
    listener_obj: u16,
    event_obj: u16,
    values_arr: u16,
    value: f32,
) -> Result<(), JvmError> {
    // Rewrite the recycled event's mutable payload (see Registration): only
    // values[0] and timestamp change per tick, so delivery allocates nothing.
    heap.arrays.store(values_arr, 0, value.to_bits() as i32);

    let ts = crate::hal::system_clock::elapsed_realtime_nanos();
    heap.objects
        .set_field(event_obj, event_fields::TIMESTAMP, Value::Long(ts))
        .ok_or(JvmError::StackOverflow)?;

    // Invoke listener.onSensorChanged(event)
    let listener_class = heap.objects.class_name(listener_obj).unwrap_or("unknown");
    let static_class: &'static str =
        unsafe { core::mem::transmute::<&str, &'static str>(listener_class) };

    jvm.invoke_instance_with_args(
        static_class,
        "onSensorChanged",
        listener_obj,
        &[Value::ObjectRef(event_obj)],
        heap,
        handler,
    )
}

/// Latest env-cluster snapshot for the drain.
#[cfg(all(not(test), any(any_sensor, feature = "sim")))]
fn read_env_snapshot() -> Option<EnvSnapshot> {
    mailbox::read_env()
}

/// Sensor-less device builds: no sampler exists — serve zeros at cadence,
/// preserving the pre-task behavior on boards without `[[sensor]]` entries.
#[cfg(all(not(test), not(any_sensor), not(feature = "sim")))]
fn read_env_snapshot() -> Option<EnvSnapshot> {
    Some(EnvSnapshot::default())
}

/// Latest optical-cluster snapshot for the drain.
#[cfg(all(not(test), any(any_sensor, feature = "sim")))]
fn read_optical_snapshot() -> Option<OpticalSnapshot> {
    mailbox::read_optical()
}

#[cfg(all(not(test), not(any_sensor), not(feature = "sim")))]
fn read_optical_snapshot() -> Option<OpticalSnapshot> {
    Some(OpticalSnapshot::default())
}
