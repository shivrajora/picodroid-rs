use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::super::fields;
use super::super::helpers::{alloc_peripheral_with_id, extract_device_name};

/// Parses a "GPxx" name string and allocates a Gpio object on the heap.
/// args[0] = PeripheralManager ObjectRef (receiver), args[1] = Reference to "GPxx" string
pub fn open_gpio(
    args: &[Value],
    strings: &StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let name = extract_device_name(args, strings)?;

    // Parse "GPxx" → pin number
    let pin_str = name.strip_prefix("GP").ok_or(JvmError::InvalidReference)?;
    let mut pin: u8 = 0;
    for c in pin_str.chars() {
        let d = (c as u8).wrapping_sub(b'0');
        if d > 9 {
            return Err(JvmError::InvalidReference);
        }
        pin = pin.wrapping_mul(10).wrapping_add(d);
    }

    let obj_idx = alloc_peripheral_with_id(objects, "picodroid/pio/Gpio", fields::gpio::PIN, pin)?;

    Ok(Some(Value::ObjectRef(obj_idx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::{heap::StringTable, object_heap::ObjectHeap, types::Value};

    static GP25: &[u8] = b"GP25";
    static GP0: &[u8] = b"GP0";
    static NO_PREFIX: &[u8] = b"XX25";
    static BAD_DIGIT: &[u8] = b"GP!5";

    #[test]
    fn open_gpio_gp25_sets_pin_25() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP25).expect("intern GP25");
        let result = open_gpio(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::gpio::PIN),
            Some(Value::Int(25))
        );
    }

    #[test]
    fn open_gpio_gp0_sets_pin_0() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP0).expect("intern GP0");
        let result = open_gpio(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::gpio::PIN),
            Some(Value::Int(0))
        );
    }

    #[test]
    fn open_gpio_invalid_ref_type_returns_error() {
        let strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let result = open_gpio(&[Value::Null, Value::Int(0)], &strings, &mut objects);
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_gpio_no_gp_prefix_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(NO_PREFIX).expect("intern NO_PREFIX");
        let result = open_gpio(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_gpio_bad_digit_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(BAD_DIGIT).expect("intern BAD_DIGIT");
        let result = open_gpio(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }
}
