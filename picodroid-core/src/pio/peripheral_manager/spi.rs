// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::super::fields;
use super::super::helpers::{alloc_peripheral_with_id, extract_device_name, parse_bus_id};

/// Parses a "SPIx" name string, allocates a SpiDevice object with default config (1 MHz, MODE_0),
/// and initializes the hardware.
/// args[0] = PeripheralManager ObjectRef (receiver), args[1] = Reference to "SPIx" string
pub fn open_spi(
    args: &[Value],
    strings: &StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let name = extract_device_name(args, strings)?;
    let spi_id = parse_bus_id(name, "SPI")?;

    let obj_idx = alloc_peripheral_with_id(
        objects,
        "picodroid/pio/SpiDevice",
        fields::spi::SPI_ID,
        spi_id,
    )?;

    // Store default config: frequency_hz=1_000_000, mode=0 (MODE_0)
    objects
        .set_field(obj_idx, fields::spi::FREQUENCY_HZ, Value::Int(1_000_000))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::spi::MODE, Value::Int(0))
        .ok_or(JvmError::StackOverflow)?;

    // Initialize hardware: GPIO function select + SPI enable at 1 MHz, MODE_0
    #[cfg(not(test))]
    super::super::spi::init(spi_id);

    Ok(Some(Value::ObjectRef(obj_idx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::{heap::StringTable, object_heap::ObjectHeap, types::Value};

    static SPI0_NAME: &[u8] = b"SPI0";
    static SPI1_NAME: &[u8] = b"SPI1";
    static SPI_BAD_ID: &[u8] = b"SPI2";
    static SPI_NO_PREFIX: &[u8] = b"I2C0";

    #[test]
    fn open_spi_spi0_sets_id_0_and_defaults() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(SPI0_NAME).expect("intern SPI0");
        let result = open_spi(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::spi::SPI_ID),
            Some(Value::Int(0))
        );
        assert_eq!(
            objects.get_field(idx, fields::spi::FREQUENCY_HZ),
            Some(Value::Int(1_000_000))
        );
        assert_eq!(
            objects.get_field(idx, fields::spi::MODE),
            Some(Value::Int(0))
        );
    }

    #[test]
    fn open_spi_spi1_sets_id_1() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(SPI1_NAME).expect("intern SPI1");
        let result = open_spi(
            &[Value::ObjectRef(0), Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        let idx = match result {
            Ok(Some(Value::ObjectRef(i))) => i,
            other => panic!("expected Ok(Some(ObjectRef(...))), got {:?}", other),
        };
        assert_eq!(
            objects.get_field(idx, fields::spi::SPI_ID),
            Some(Value::Int(1))
        );
    }

    #[test]
    fn open_spi_bad_id_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(SPI_BAD_ID).expect("intern SPI2");
        let result = open_spi(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_spi_no_prefix_returns_error() {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let ref_idx = strings.intern(SPI_NO_PREFIX).expect("intern I2C0");
        let result = open_spi(
            &[Value::Null, Value::Reference(ref_idx)],
            &strings,
            &mut objects,
        );
        assert_eq!(result, Err(JvmError::InvalidReference));
    }

    #[test]
    fn open_spi_invalid_ref_type_returns_error() {
        let strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let result = open_spi(&[Value::Null, Value::Int(0)], &strings, &mut objects);
        assert_eq!(result, Err(JvmError::InvalidReference));
    }
}
