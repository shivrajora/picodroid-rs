use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::super::fields;
use super::super::helpers::{alloc_peripheral_with_id, extract_device_name, parse_bus_id};

/// Parses a "UARTx" name string, allocates a UartDevice object with default config (9600 8N1),
/// and initializes the hardware.
/// args[0] = PeripheralManager ObjectRef (receiver), args[1] = Reference to "UARTx" string
pub fn open_uart(
    args: &[Value],
    strings: &StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let name = extract_device_name(args, strings)?;
    let uart_id = parse_bus_id(name, "UART")?;

    let obj_idx = alloc_peripheral_with_id(
        objects,
        "picodroid/pio/UartDevice",
        fields::uart::UART_ID,
        uart_id,
    )?;

    // Store default config: 9600 baud, 8 data bits, no parity, 1 stop bit, no hw flow
    objects
        .set_field(obj_idx, fields::uart::BAUDRATE, Value::Int(9600))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::uart::DATA_SIZE, Value::Int(8))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::uart::PARITY, Value::Int(0))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::uart::STOP_BITS, Value::Int(1))
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj_idx, fields::uart::HW_FLOW, Value::Int(0))
        .ok_or(JvmError::StackOverflow)?;

    // Initialize hardware: GPIO function select + UART enable with defaults
    #[cfg(not(test))]
    super::super::uart::init(uart_id);

    Ok(Some(Value::ObjectRef(obj_idx)))
}
