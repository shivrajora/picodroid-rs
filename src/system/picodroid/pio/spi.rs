use pico_jvm::{
    array_heap::ArrayHeap,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

// -------------------------------------------------------------------
// Object field layout for picodroid/pio/SpiDevice in ObjectHeap:
//   field 0: spi_id       (Int: 0=SPI0, 1=SPI1)
//   field 1: frequency_hz (Int, default 1_000_000)
//   field 2: mode         (Int, default 0 = MODE_0: CPOL=0, CPHA=0)
// -------------------------------------------------------------------

#[cfg(not(feature = "sim"))]
#[path = "spi/real.rs"]
mod platform;
#[cfg(feature = "sim")]
#[path = "spi/sim.rs"]
mod platform;

fn extract_obj_idx(args: &[Value]) -> Result<u16, JvmError> {
    match args.first() {
        Some(Value::ObjectRef(idx)) => Ok(*idx),
        _ => Err(JvmError::InvalidReference),
    }
}

fn extract_spi_id(args: &[Value], objects: &ObjectHeap) -> Result<u8, JvmError> {
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

/// Configure GPIO pins for SPI function and start the controller at 1 MHz, MODE_0.
/// Called once from `peripheral_manager::open_spi()`.
pub fn init(spi_id: u8) {
    platform::init(spi_id);
}

pub fn set_frequency_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let hz = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, 1, Value::Int(hz as i32))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, 0, 0) as u8;
    let mode = read_field(objects, idx, 2, 0) as u32;
    platform::reconfigure(id, hz, mode);
    Ok(None)
}

pub fn set_mode_native(
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let idx = extract_obj_idx(args)?;
    let mode = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    objects
        .set_field(idx, 2, Value::Int(mode as i32))
        .ok_or(JvmError::StackOverflow)?;
    let id = read_field(objects, idx, 0, 0) as u8;
    let hz = read_field(objects, idx, 1, 1_000_000) as u32;
    platform::reconfigure(id, hz, mode);
    Ok(None)
}

/// Full-duplex transfer. args: [this, ArrayRef(tx), ArrayRef(rx), Int(len)]
/// Writes tx[0..len-1] and stores received bytes into rx[0..len-1].
/// Returns Int(len) on success.
pub fn transfer_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let spi_id = extract_spi_id(args, objects)?;
    let tx_idx = match args.get(1) {
        Some(Value::ArrayRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let rx_idx = match args.get(2) {
        Some(Value::ArrayRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let len = match args.get(3) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    let result = platform::transfer(spi_id, tx_idx, rx_idx, len, arrays);
    Ok(Some(Value::Int(result)))
}

/// Write-only transfer. args: [this, ArrayRef(data), Int(len)]
/// Sends data[0..len-1] and discards received bytes.
/// Returns Int(len) on success.
pub fn write_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let spi_id = extract_spi_id(args, objects)?;
    let data_idx = match args.get(1) {
        Some(Value::ArrayRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let len = match args.get(2) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };
    let result = platform::write(spi_id, data_idx, len, arrays);
    Ok(Some(Value::Int(result)))
}
