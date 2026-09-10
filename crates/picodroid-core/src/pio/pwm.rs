// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub use super::fields::pwm as fields;
use super::helpers::extract_obj_idx;

use crate::hal::pwm as platform;

fn read_pin(objects: &ObjectHeap, idx: u16) -> u8 {
    match objects.get_field(idx, fields::PIN) {
        Some(Value::Int(v)) => v as u8,
        _ => 0,
    }
}

fn read_freq(objects: &ObjectHeap, idx: u16) -> f64 {
    match objects.get_field(idx, fields::FREQUENCY_HZ) {
        Some(Value::Double(v)) => v,
        _ => 1000.0,
    }
}

fn read_duty(objects: &ObjectHeap, idx: u16) -> f64 {
    match objects.get_field(idx, fields::DUTY_CYCLE) {
        Some(Value::Double(v)) => v,
        _ => 0.0,
    }
}

fn read_enabled(objects: &ObjectHeap, idx: u16) -> bool {
    match objects.get_field(idx, fields::ENABLED) {
        Some(Value::Int(v)) => v != 0,
        _ => false,
    }
}

/// Configure GPIO pin for PWM function and apply default settings (1 kHz, 0% duty, disabled).
/// Called once from `peripheral_manager::open_pwm()`.
pub fn init(pin: u8) {
    platform::init(pin);
}

/// args: [this, Int(enabled)]  (JVM encodes boolean as int 0/1)
pub fn set_enabled_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let enabled = match args.get(1) {
        Some(Value::Int(v)) => *v != 0,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::ENABLED, Value::Int(enabled as i32))
        .ok_or(JvmError::StackOverflow)?;
    let pin = read_pin(objects, idx);
    let freq = read_freq(objects, idx);
    let duty = read_duty(objects, idx);
    platform::apply(pin, freq, duty, enabled);
    Ok(None)
}

/// args: [this, Double(freqHz)]
pub fn set_frequency_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let freq = match args.get(1) {
        Some(Value::Double(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::FREQUENCY_HZ, Value::Double(freq))
        .ok_or(JvmError::StackOverflow)?;
    let pin = read_pin(objects, idx);
    let duty = read_duty(objects, idx);
    let enabled = read_enabled(objects, idx);
    platform::apply(pin, freq, duty, enabled);
    Ok(None)
}

/// args: [this, Double(dutyCycle)]
pub fn set_duty_cycle_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let duty = match args.get(1) {
        Some(Value::Double(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, fields::DUTY_CYCLE, Value::Double(duty))
        .ok_or(JvmError::StackOverflow)?;
    let pin = read_pin(objects, idx);
    let freq = read_freq(objects, idx);
    let enabled = read_enabled(objects, idx);
    platform::apply(pin, freq, duty, enabled);
    Ok(None)
}
