use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::super::fields;

/// Parses a "GPx" name string, allocates a Pwm object with default config (1 kHz, 0% duty,
/// disabled), and initializes the hardware.
/// args[0] = PeripheralManager ObjectRef (receiver), args[1] = Reference to "GPx" string
pub fn open_pwm(
    args: &[Value],
    strings: &StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let name_ref = match args.get(1) {
        Some(Value::Reference(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let name = strings
        .resolve(name_ref)
        .ok_or(JvmError::InvalidReference)?;

    // Parse "GPx" → pin number 0–29
    let pin_str = name.strip_prefix("GP").ok_or(JvmError::InvalidReference)?;
    let pin: u8 = pin_str
        .parse::<u8>()
        .ok()
        .filter(|&p| p <= 29)
        .ok_or(JvmError::InvalidReference)?;

    let obj_idx = objects
        .alloc("picodroid/pio/Pwm")
        .ok_or(JvmError::StackOverflow)?;

    // Store default config fields
    objects
        .set_field(obj_idx, fields::pwm::PIN, Value::Int(pin as i32))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::pwm::FREQUENCY_HZ, Value::Double(1000.0))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::pwm::DUTY_CYCLE, Value::Double(0.0))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::pwm::ENABLED, Value::Int(0))
        .ok_or(JvmError::StackOverflow)?;

    // Initialize hardware: GPIO function select + PWM slice default config
    #[cfg(not(test))]
    super::super::pwm::init(pin);

    Ok(Some(Value::ObjectRef(obj_idx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::{heap::StringTable, object_heap::ObjectHeap, types::Value};

    static GP0: &[u8] = b"GP0";
    static GP25: &[u8] = b"GP25";
    static GP29: &[u8] = b"GP29";
    static GP30: &[u8] = b"GP30";
    static SPI0: &[u8] = b"SPI0";
    static EMPTY: &[u8] = b"";

    #[test]
    fn open_pwm_gp0_sets_pin_0_and_defaults() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP0).expect("intern GP0");
        let result = open_pwm(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::pwm::PIN),
            Some(Value::Int(0))
        );
        assert_eq!(
            objects.get_field(idx, fields::pwm::FREQUENCY_HZ),
            Some(Value::Double(1000.0))
        );
        assert_eq!(
            objects.get_field(idx, fields::pwm::DUTY_CYCLE),
            Some(Value::Double(0.0))
        );
        assert_eq!(
            objects.get_field(idx, fields::pwm::ENABLED),
            Some(Value::Int(0))
        );
    }

    #[test]
    fn open_pwm_gp25_sets_pin_25() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP25).expect("intern GP25");
        let result = open_pwm(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::pwm::PIN),
            Some(Value::Int(25))
        );
    }

    #[test]
    fn open_pwm_gp29_sets_pin_29() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP29).expect("intern GP29");
        let result = open_pwm(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert!(matches!(result, Ok(Some(Value::ObjectRef(_)))));
    }

    #[test]
    fn open_pwm_gp30_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP30).expect("intern GP30");
        let result = open_pwm(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_pwm_wrong_prefix_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(SPI0).expect("intern SPI0");
        let result = open_pwm(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_pwm_empty_name_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(EMPTY).expect("intern empty");
        let result = open_pwm(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_pwm_invalid_ref_type_returns_error() {
        let strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let result = open_pwm(&[Value::Null, Value::Int(0)], &strings, &mut objects);
        assert_eq!(result, Err(JvmError::InvalidReference));
    }
}
