use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::super::fields;

/// Parses an "I2Cx" name string, allocates an I2cDevice object with default config (100 kHz),
/// and initializes the hardware.
/// args[0] = PeripheralManager ObjectRef (receiver), args[1] = Reference to "I2Cx" string
pub fn open_i2c(
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

    // Parse "I2C0" → 0, "I2C1" → 1
    let id_str = name.strip_prefix("I2C").ok_or(JvmError::InvalidReference)?;
    let i2c_id: u8 = match id_str {
        "0" => 0,
        "1" => 1,
        _ => return Err(JvmError::InvalidReference),
    };

    let obj_idx = objects
        .alloc("picodroid/pio/I2cDevice")
        .ok_or(JvmError::StackOverflow)?;

    // Store config fields: i2c_id, speed_hz=100_000 (standard speed default)
    objects
        .set_field(obj_idx, fields::i2c::I2C_ID, Value::Int(i2c_id as i32))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::i2c::SPEED_HZ, Value::Int(100_000))
        .ok_or(JvmError::StackOverflow)?;

    // Initialize hardware: GPIO function select + I2C enable at 100 kHz
    #[cfg(not(test))]
    super::super::i2c::init(i2c_id);

    Ok(Some(Value::ObjectRef(obj_idx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::{heap::StringTable, object_heap::ObjectHeap, types::Value};

    static I2C0_NAME: &[u8] = b"I2C0";
    static I2C1_NAME: &[u8] = b"I2C1";
    static I2C_BAD_ID: &[u8] = b"I2C2";
    static I2C_NO_PREFIX: &[u8] = b"SPI0";

    #[test]
    fn open_i2c_i2c0_sets_id_0_and_default_speed() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(I2C0_NAME).expect("intern I2C0");
        let result = open_i2c(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::i2c::I2C_ID),
            Some(Value::Int(0))
        );
        assert_eq!(
            objects.get_field(idx, fields::i2c::SPEED_HZ),
            Some(Value::Int(100_000))
        );
    }

    #[test]
    fn open_i2c_i2c1_sets_id_1() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(I2C1_NAME).expect("intern I2C1");
        let result = open_i2c(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::i2c::I2C_ID),
            Some(Value::Int(1))
        );
    }

    #[test]
    fn open_i2c_bad_id_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(I2C_BAD_ID).expect("intern I2C2");
        let result = open_i2c(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_i2c_no_prefix_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(I2C_NO_PREFIX).expect("intern SPI0");
        let result = open_i2c(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_i2c_invalid_ref_type_returns_error() {
        let strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let result = open_i2c(&[Value::Null, Value::Int(0)], &strings, &mut objects);
        assert_eq!(result, Err(JvmError::InvalidReference));
    }
}
