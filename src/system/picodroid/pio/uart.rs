use pico_jvm::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

// -------------------------------------------------------------------
// Object field layout for picodroid/pio/UartDevice in ObjectHeap:
//   field 0: uart_id   (Int: 0=UART0, 1=UART1)
//   field 1: baudrate  (Int, default 9600)
//   field 2: data_size (Int, default 8)
//   field 3: parity    (Int, default 0=NONE)
//   field 4: stop_bits (Int, default 1)
//   field 5: hw_flow   (Int, default 0=NONE)
// -------------------------------------------------------------------

#[cfg(not(feature = "sim"))]
#[path = "uart/real.rs"]
mod platform;
#[cfg(feature = "sim")]
#[path = "uart/sim.rs"]
mod platform;

fn extract_obj_idx(args: &[Value]) -> Result<u16, JvmError> {
    match args.first() {
        Some(Value::ObjectRef(idx)) => Ok(*idx),
        _ => Err(JvmError::InvalidReference),
    }
}

fn extract_uart_id(args: &[Value], objects: &ObjectHeap) -> Result<u8, JvmError> {
    let idx = extract_obj_idx(args)?;
    match objects.get_field(idx, 0) {
        Some(Value::Int(id)) => Ok(id as u8),
        _ => Err(JvmError::InvalidReference),
    }
}

fn read_field(objects: &ObjectHeap, idx: u16, field: usize, default: i32) -> i32 {
    match objects.get_field(idx, field) {
        Some(Value::Int(v)) => v,
        _ => default,
    }
}

fn get_config(objects: &ObjectHeap, idx: u16) -> (i32, i32, i32, i32, i32) {
    (
        read_field(objects, idx, 1, 9600),
        read_field(objects, idx, 2, 8),
        read_field(objects, idx, 3, 0),
        read_field(objects, idx, 4, 1),
        read_field(objects, idx, 5, 0),
    )
}

/// Configure GPIO pins for UART function and start the UART with defaults (9600 8N1).
/// Called once from `peripheral_manager::open_uart()`.
pub fn init(uart_id: u8) {
    platform::init(uart_id);
}

pub fn set_baudrate_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let rate = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, 1, Value::Int(rate))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, 0, 0) as u8;
    let (_, data_size, parity, stop_bits, hw_flow) = get_config(objects, idx);
    platform::reconfigure(id, rate, data_size, parity, stop_bits, hw_flow);
    Ok(None)
}

pub fn set_data_size_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let size = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, 2, Value::Int(size))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, 0, 0) as u8;
    let (baudrate, _, parity, stop_bits, hw_flow) = get_config(objects, idx);
    platform::reconfigure(id, baudrate, size, parity, stop_bits, hw_flow);
    Ok(None)
}

pub fn set_parity_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let mode = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, 3, Value::Int(mode))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, 0, 0) as u8;
    let (baudrate, data_size, _, stop_bits, hw_flow) = get_config(objects, idx);
    platform::reconfigure(id, baudrate, data_size, mode, stop_bits, hw_flow);
    Ok(None)
}

pub fn set_stop_bits_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let bits = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, 4, Value::Int(bits))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, 0, 0) as u8;
    let (baudrate, data_size, parity, _, hw_flow) = get_config(objects, idx);
    platform::reconfigure(id, baudrate, data_size, parity, bits, hw_flow);
    Ok(None)
}

pub fn set_hw_flow_ctrl_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let mode = match args.get(1) {
        Some(Value::Int(v)) => *v,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, 5, Value::Int(mode))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, 0, 0) as u8;
    let (baudrate, data_size, parity, stop_bits, _) = get_config(objects, idx);
    platform::reconfigure(id, baudrate, data_size, parity, stop_bits, mode);
    Ok(None)
}

/// Blocking write of a single byte. Returns `Some(Int(1))` on success.
pub fn write_byte_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let uart_id = extract_uart_id(args, objects)?;
    let byte = match args.get(1) {
        Some(Value::Int(v)) => *v as u8,
        _ => return Err(JvmError::InvalidReference),
    };
    platform::write_byte(uart_id, byte);
    Ok(Some(Value::Int(1)))
}

/// Non-blocking read of a single byte. Returns `Some(Int(-1))` if RX FIFO is empty.
pub fn read_byte_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let uart_id = extract_uart_id(args, objects)?;
    Ok(Some(Value::Int(platform::read_byte(uart_id))))
}
