use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::super::fields;
use super::super::helpers::{alloc_peripheral_with_id, extract_device_name};

/// Parses a "GPx" name string (GP26–GP29), allocates an Adc object, and initializes the hardware.
/// args[0] = PeripheralManager ObjectRef (receiver), args[1] = Reference to "GPx" string
pub fn open_adc(
    args: &[Value],
    strings: &StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let name = extract_device_name(args, strings)?;

    // Parse "GP26"–"GP29" → pin 26–29
    let pin_str = name.strip_prefix("GP").ok_or(JvmError::InvalidReference)?;
    let pin: u8 = pin_str.parse().map_err(|_| JvmError::InvalidReference)?;
    if !(26..=29).contains(&pin) {
        return Err(JvmError::InvalidReference);
    }

    let obj_idx = alloc_peripheral_with_id(objects, "picodroid/pio/Adc", fields::adc::PIN, pin)?;

    // Initialize hardware: configure GPIO pin for analog input and enable ADC
    #[cfg(not(test))]
    super::super::adc::init(pin);

    Ok(Some(Value::ObjectRef(obj_idx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::{heap::StringTable, object_heap::ObjectHeap, types::Value};

    static GP26: &[u8] = b"GP26";
    static GP27: &[u8] = b"GP27";
    static GP28: &[u8] = b"GP28";
    static GP29: &[u8] = b"GP29";
    static GP25: &[u8] = b"GP25";
    static GP30: &[u8] = b"GP30";
    static SPI0: &[u8] = b"SPI0";
    static EMPTY: &[u8] = b"";

    #[test]
    fn open_adc_gp26_sets_pin_26() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP26).expect("intern GP26");
        let result = open_adc(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::adc::PIN),
            Some(Value::Int(26))
        );
    }

    #[test]
    fn open_adc_gp27_sets_pin_27() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP27).expect("intern GP27");
        let result = open_adc(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::adc::PIN),
            Some(Value::Int(27))
        );
    }

    #[test]
    fn open_adc_gp28_sets_pin_28() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP28).expect("intern GP28");
        let result = open_adc(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert!(matches!(result, Ok(Some(Value::ObjectRef(_)))));
    }

    #[test]
    fn open_adc_gp29_sets_pin_29() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP29).expect("intern GP29");
        let result = open_adc(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert!(matches!(result, Ok(Some(Value::ObjectRef(_)))));
    }

    #[test]
    fn open_adc_gp25_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP25).expect("intern GP25");
        let result = open_adc(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_adc_gp30_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(GP30).expect("intern GP30");
        let result = open_adc(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_adc_wrong_prefix_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(SPI0).expect("intern SPI0");
        let result = open_adc(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_adc_empty_name_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(EMPTY).expect("intern empty");
        let result = open_adc(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_adc_invalid_ref_type_returns_error() {
        let strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let result = open_adc(&[Value::Null, Value::Int(0)], &strings, &mut objects);
        assert_eq!(result, Err(JvmError::InvalidReference));
    }
}
