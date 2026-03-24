use pico_jvm::{
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::super::fields;

/// Parses a "UARTx" name string, allocates a UartDevice object with default config (9600 8N1),
/// and initializes the hardware.
/// args[0] = PeripheralManager ObjectRef (receiver), args[1] = Reference to "UARTx" string
pub fn open_uart(
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

    // Parse "UART0" → 0, "UART1" → 1
    let id_str = name
        .strip_prefix("UART")
        .ok_or(JvmError::InvalidReference)?;
    let uart_id: u8 = match id_str {
        "0" => 0,
        "1" => 1,
        _ => return Err(JvmError::InvalidReference),
    };

    let obj_idx = objects
        .alloc("picodroid/pio/UartDevice")
        .ok_or(JvmError::StackOverflow)?;

    // Store config fields with defaults: uart_id, 9600 baud, 8 data bits, no parity, 1 stop bit, no hw flow
    objects
        .set_field(obj_idx, fields::uart::UART_ID, Value::Int(uart_id as i32))
        .ok_or(JvmError::StackOverflow)?;
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
