// SPDX-License-Identifier: GPL-3.0-only
//! SensorManager native method implementations.
//!
//! Manages sensor registrations and event delivery. For sim builds, sensors are
//! polled synchronously from the main-loop tick. On hardware, a FreeRTOS task
//! will sample and queue events (deferred to a follow-up commit).

use pico_jvm::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

// Sim builds still include the table (the sensor cfgs are board-driven) but
// only the hardware sampling paths consume it — sim sensors are synthesized.
#[cfg(any(sensor_bme688, sensor_ltr559))]
#[cfg_attr(feature = "sim", allow(dead_code))]
mod sensor_table {
    include!(concat!(env!("OUT_DIR"), "/sensor_table.rs"));
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
    period_ticks: u32,
    ticks_until_due: u32,
}

struct SensorState {
    registrations: [Option<Registration>; MAX_REGISTRATIONS],
    sensor_objs: [Option<u16>; 6], // temp, hum, press, gas, light, proximity
    tick_count: u64,
}

impl SensorState {
    const fn new() -> Self {
        Self {
            registrations: [const { None }; MAX_REGISTRATIONS],
            sensor_objs: [None; 6],
            tick_count: 0,
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

const NUM_SENSOR_TYPES: usize = 6;

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

pub fn register_listener(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
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
            return Ok(Some(Value::Int(1))); // true
        }
    }

    // Find empty slot
    for slot in st.registrations.iter_mut() {
        if slot.is_none() {
            *slot = Some(Registration {
                listener_obj,
                sensor_type,
                sensor_obj,
                period_ticks,
                ticks_until_due: 0,
            });
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
    for slot in st.registrations.iter_mut() {
        if let Some(reg) = slot {
            if reg.listener_obj == listener_obj {
                *slot = None;
            }
        }
    }
    Ok(None)
}

// ── Main-loop event drain ───────────────────────────────────────────────────

/// Poll sensors and deliver SensorEvent callbacks for any due registrations.
/// Called once per main-loop tick (~16 ms).
#[cfg(not(test))]
pub fn drain_sensor_events(
    jvm: &mut pico_jvm::Jvm,
    heap: &mut pico_jvm::SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    let st = state();
    st.tick_count += 1;

    // Collect which sensor types are due this tick
    let mut types_due = [false; NUM_SENSOR_TYPES];
    for reg in st.registrations.iter_mut().flatten() {
        if reg.ticks_until_due == 0 {
            if let Some(idx) = sensor_type_index(reg.sensor_type) {
                types_due[idx] = true;
            }
            reg.ticks_until_due = reg.period_ticks;
        }
        reg.ticks_until_due = reg.ticks_until_due.saturating_sub(1);
    }

    if !types_due.iter().any(|&d| d) {
        return;
    }

    // Sample each sensor cluster only if at least one of its types is due.
    let bme_due = types_due[0..4].iter().any(|&d| d);
    let ltr_due = types_due[4..6].iter().any(|&d| d);
    #[cfg(not(feature = "sim"))]
    defmt::debug!(
        "sensors: drain tick={} bme_due={} ltr_due={}",
        st.tick_count,
        bme_due,
        ltr_due
    );
    let env = if bme_due {
        sample_bme688(st.tick_count)
    } else {
        SensorReading::default()
    };
    let optical = if ltr_due {
        sample_ltr559(st.tick_count)
    } else {
        OpticalReading::default()
    };

    // Deliver events for each due registration
    let mut delivered = 0u32;
    for i in 0..MAX_REGISTRATIONS {
        let reg = match &st.registrations[i] {
            Some(r) => r,
            None => continue,
        };
        let type_idx = match sensor_type_index(reg.sensor_type) {
            Some(idx) if types_due[idx] => idx,
            _ => continue,
        };

        let value = match type_idx {
            0 => env.temp_centi_c as f32 / 100.0,
            1 => env.hum_milli_pct as f32 / 1000.0,
            2 => env.press_pa as f32 / 100.0, // hPa
            3 => env.gas_ohm as f32,
            4 => optical.lux_milli as f32 / 1000.0,
            5 => optical.proximity_raw as f32,
            _ => continue,
        };

        let listener_obj = reg.listener_obj;
        let sensor_obj = reg.sensor_obj;

        delivered += 1;
        if let Err(e) = deliver_event(jvm, heap, handler, listener_obj, sensor_obj, value) {
            #[cfg(not(feature = "sim"))]
            defmt::warn!("sensors: deliver_event err");
            #[cfg(feature = "sim")]
            eprintln!("[sim] sensor event delivery error: {e}");
            let _ = e;
        }
    }
    #[cfg(not(feature = "sim"))]
    defmt::debug!("sensors: drain delivered {} regs", delivered);
    #[cfg(feature = "sim")]
    let _ = delivered;
}

#[cfg(not(test))]
fn deliver_event(
    jvm: &mut pico_jvm::Jvm,
    heap: &mut pico_jvm::SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
    listener_obj: u16,
    sensor_obj: u16,
    value: f32,
) -> Result<(), JvmError> {
    // Allocate SensorEvent
    let event_obj = heap
        .objects
        .alloc(crate::shrink_names::shrink_class(
            "picodroid/hardware/SensorEvent",
        ))
        .ok_or(JvmError::StackOverflow)?;

    // Set event.sensor
    heap.objects
        .set_field(
            event_obj,
            event_fields::SENSOR,
            Value::ObjectRef(sensor_obj),
        )
        .ok_or(JvmError::StackOverflow)?;

    // Allocate float[1] for values
    let arr = heap
        .arrays
        .alloc(pico_jvm::array_heap::ATYPE_FLOAT, 1)
        .ok_or(JvmError::StackOverflow)?;
    heap.arrays.store(arr, 0, value.to_bits() as i32);
    heap.objects
        .set_field(event_obj, event_fields::VALUES, Value::ArrayRef(arr))
        .ok_or(JvmError::StackOverflow)?;

    // Set accuracy = 3 (ACCURACY_HIGH)
    heap.objects
        .set_field(event_obj, event_fields::ACCURACY, Value::Int(3))
        .ok_or(JvmError::StackOverflow)?;

    // Set timestamp
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

#[derive(Default)]
struct SensorReading {
    temp_centi_c: i32,
    hum_milli_pct: u32,
    press_pa: u32,
    gas_ohm: u32,
}

/// Sim path: bypass the I²C fake (which compensates to zeros because its
/// calibration registers are all zero) and emit a slow triangle wave around
/// realistic indoor values. The wobble proves the UI is re-rendering on each
/// sensor tick and lets threshold-breach animations be exercised without
/// rebuilding (raise `Theme`-driven thresholds, watch the tile pulse).
///
/// Period: ~200 ticks (~3 s at the 16 ms main-loop cadence). Amplitudes are
/// chosen to stay inside the default threshold band so steady-state is calm.
#[cfg(all(not(test), feature = "sim"))]
fn sample_bme688(tick: u64) -> SensorReading {
    let phase = (tick % 200) as f32 / 100.0 - 1.0; // triangle in [-1, 1)
    SensorReading {
        temp_centi_c: (2200.0 + phase * 50.0) as i32, // 22.0 ± 0.5 °C
        hum_milli_pct: (45_000.0 + phase * 2_000.0) as u32, // 45 ± 2 %
        press_pa: (101_325.0 + phase * 100.0) as u32, // 1013.25 ± 1 hPa
        gas_ohm: (50_000.0 + phase * 5_000.0) as u32, // 50 kΩ ± 5 kΩ
    }
}

/// Hardware path: drive the real BME688 over I²C, oversampled to TPHG.
#[cfg(all(not(test), not(feature = "sim")))]
fn sample_bme688(_tick: u64) -> SensorReading {
    #[cfg(sensor_bme688)]
    {
        use crate::drivers::bme688::I2cBus;

        struct HalI2c {
            bus_id: u8,
        }
        impl I2cBus for HalI2c {
            fn write(&mut self, addr: u8, data: &[u8]) -> i32 {
                crate::hal::i2c::write_slice(self.bus_id, addr, data)
            }
            fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32 {
                crate::hal::i2c::read_slice(self.bus_id, addr, buf)
            }
        }

        static mut BME_DRIVER: Option<crate::drivers::bme688::Bme688<HalI2c>> = None;

        let bme = unsafe {
            // Go through a raw pointer rather than referencing the static
            // directly, which would trip `static_mut_refs` (Rust 2024 compat).
            let bme_ptr = &raw mut BME_DRIVER;
            if (*bme_ptr).is_none() {
                for cfg in sensor_table::SENSORS {
                    if matches!(cfg.kind, sensor_table::SensorKind::Bme688) {
                        crate::hal::i2c::init(cfg.bus_id);
                        let bus = HalI2c { bus_id: cfg.bus_id };
                        if let Ok(driver) = crate::drivers::bme688::Bme688::new(bus, cfg.addr) {
                            *bme_ptr = Some(driver);
                        }
                        break;
                    }
                }
            }
            &mut *bme_ptr
        };

        match bme {
            Some(driver) => {
                use embedded_hal::delay::DelayNs;
                #[cfg(not(feature = "sim"))]
                defmt::debug!("bme: trigger");
                let _ = driver.trigger_forced();
                // BME688 forced-mode TPHG conversion is ~10–15 ms at 1x
                // oversampling; gas heater adds variable extra time. Wait
                // the typical worst case before polling so we don't tight-
                // spin against an in-progress measurement.
                #[cfg(feature = "sim")]
                let mut delay = crate::hal::delay::SimDelay::new();
                #[cfg(not(feature = "sim"))]
                let mut delay = crate::hal::delay::RpDelay::new();
                delay.delay_ms(20);
                let ready = driver.poll_ready(5);
                #[cfg(not(feature = "sim"))]
                defmt::debug!("bme: poll_ready={}", ready);
                if ready {
                    let r = driver.read_compensated().unwrap_or_default();
                    #[cfg(not(feature = "sim"))]
                    defmt::debug!(
                        "bme: temp={} hum={} press={} gas={}",
                        r.temp_centi_c,
                        r.hum_milli_pct,
                        r.press_pa,
                        r.gas_ohm
                    );
                    SensorReading {
                        temp_centi_c: r.temp_centi_c,
                        hum_milli_pct: r.hum_milli_pct,
                        press_pa: r.press_pa,
                        gas_ohm: r.gas_ohm,
                    }
                } else {
                    SensorReading::default()
                }
            }
            None => SensorReading::default(),
        }
    }
    #[cfg(not(sensor_bme688))]
    {
        SensorReading::default()
    }
}

#[derive(Default)]
struct OpticalReading {
    lux_milli: u32,
    proximity_raw: u16,
}

/// Sim path: no LTR559 fake exists in the sim I²C responder (it only knows
/// the BME688 at 0x77), so without this we'd permanently read 0 lx. Use the
/// same triangle wave as the BME path for visual consistency.
#[cfg(all(not(test), feature = "sim"))]
fn sample_ltr559(tick: u64) -> OpticalReading {
    let phase = (tick % 200) as f32 / 100.0 - 1.0;
    OpticalReading {
        lux_milli: (300_000.0 + phase * 50_000.0) as u32, // 300 ± 50 lx
        proximity_raw: 0,
    }
}

/// Hardware path: drive the real LTR559 over I²C for lux + proximity.
#[cfg(all(not(test), not(feature = "sim")))]
fn sample_ltr559(_tick: u64) -> OpticalReading {
    #[cfg(sensor_ltr559)]
    {
        use crate::drivers::ltr559::I2cBus;

        struct HalI2c {
            bus_id: u8,
        }
        impl I2cBus for HalI2c {
            fn write(&mut self, addr: u8, data: &[u8]) -> i32 {
                crate::hal::i2c::write_slice(self.bus_id, addr, data)
            }
            fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32 {
                crate::hal::i2c::read_slice(self.bus_id, addr, buf)
            }
        }

        static mut LTR_DRIVER: Option<crate::drivers::ltr559::Ltr559<HalI2c>> = None;

        let drv = unsafe {
            // Go through a raw pointer rather than referencing the static
            // directly, which would trip `static_mut_refs` (Rust 2024 compat).
            let ltr_ptr = &raw mut LTR_DRIVER;
            if (*ltr_ptr).is_none() {
                for cfg in sensor_table::SENSORS {
                    if matches!(cfg.kind, sensor_table::SensorKind::Ltr559) {
                        crate::hal::i2c::init(cfg.bus_id);
                        let bus = HalI2c { bus_id: cfg.bus_id };
                        if let Ok(d) = crate::drivers::ltr559::Ltr559::new(bus, cfg.addr) {
                            *ltr_ptr = Some(d);
                        }
                        break;
                    }
                }
            }
            &mut *ltr_ptr
        };

        match drv {
            Some(driver) => {
                let r = driver.measure().unwrap_or_default();
                OpticalReading {
                    lux_milli: r.lux_milli,
                    proximity_raw: r.proximity_raw,
                }
            }
            None => OpticalReading::default(),
        }
    }
    #[cfg(not(sensor_ltr559))]
    {
        OpticalReading::default()
    }
}
