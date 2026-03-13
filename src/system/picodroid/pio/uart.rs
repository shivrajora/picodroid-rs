use crate::framework::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

// RP2040 UART clock: CLK_PERI defaults to 125 MHz after boot
const PCLK_HZ: u32 = 125_000_000;

// -------------------------------------------------------------------
// Object field layout for picodroid/pio/UartDevice in ObjectHeap:
//   field 0: uart_id   (Int: 0=UART0, 1=UART1)
//   field 1: baudrate  (Int, default 9600)
//   field 2: data_size (Int, default 8)
//   field 3: parity    (Int, default 0=NONE)
//   field 4: stop_bits (Int, default 1)
//   field 5: hw_flow   (Int, default 0=NONE)
// -------------------------------------------------------------------

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

// -------------------------------------------------------------------
// Hardware helpers — apply config to UART peripheral registers
// -------------------------------------------------------------------

// Compute UARTIBRD / UARTFBRD from baud rate.
//   BRD = PCLK / (16 * baud)
//   brd_x64 = (PCLK * 4) / baud  (avoids floats; gives ibrd in upper bits, fbrd in lower 6)
fn baud_divisors(baudrate: i32) -> (u16, u8) {
    let baud = baudrate as u64;
    let brd_x64 = (PCLK_HZ as u64 * 4) / baud;
    let ibrd = (brd_x64 >> 6) as u16;
    let fbrd = (brd_x64 & 0x3F) as u8;
    (ibrd, fbrd)
}

// Apply complete UART config (disable → reprogram → re-enable).
// Uses a macro so the UART0/UART1 PAC types (which differ) can be handled
// without requiring a trait object — the register interface is identical but
// Rust sees them as distinct types.
macro_rules! apply_config {
    ($uart:expr, $baudrate:expr, $data_size:expr, $parity:expr, $stop_bits:expr, $hw_flow:expr) => {{
        // 1. Disable UART before reconfiguring line-control registers
        $uart.uartcr().write(|w| unsafe { w.bits(0) });

        // 2. Baud rate divisors
        let (ibrd, fbrd) = baud_divisors($baudrate);
        $uart
            .uartibrd()
            .write(|w| unsafe { w.baud_divint().bits(ibrd) });
        $uart
            .uartfbrd()
            .write(|w| unsafe { w.baud_divfrac().bits(fbrd) });

        // 3. Line control register (UARTLCR_H) — compute raw bits to avoid type
        //    inference issues with the 2-bit WLEN field writer.
        //    bit layout: [7]=SPS [6:5]=WLEN [4]=FEN [3]=STP2 [2]=EPS [1]=PEN [0]=BRK
        //    wlen encoding: 5→0b00, 6→0b01, 7→0b10, 8→0b11
        let wlen: u32 = (($data_size as u32).saturating_sub(5)) & 0x3;
        let stp2: u32 = if $stop_bits > 1 { 1 } else { 0 };
        let pen: u32 = if $parity != 0 { 1 } else { 0 }; // PARITY_NONE = 0
        let eps: u32 = if $parity == 1 { 1 } else { 0 }; // PARITY_EVEN = 1
        let lcrh: u32 = (wlen << 5) | (1 << 4) | (stp2 << 3) | (eps << 2) | (pen << 1);
        $uart.uartlcr_h().write(|w| unsafe { w.bits(lcrh) });

        // 4. Control register (UARTCR) — raw bits.
        //    bit layout: [15]=CTSEN [14]=RTSEN [9]=RXE [8]=TXE [0]=UARTEN
        let flow: u32 = if $hw_flow != 0 { 1 } else { 0 };
        let cr: u32 = (flow << 15) | (flow << 14) | (1 << 9) | (1 << 8) | 1;
        $uart.uartcr().write(|w| unsafe { w.bits(cr) });
    }};
}

fn reconfigure(
    uart_id: u8,
    baudrate: i32,
    data_size: i32,
    parity: i32,
    stop_bits: i32,
    hw_flow: i32,
) {
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    match uart_id {
        0 => apply_config!(&p.UART0, baudrate, data_size, parity, stop_bits, hw_flow),
        _ => apply_config!(&p.UART1, baudrate, data_size, parity, stop_bits, hw_flow),
    }
}

/// Configure GPIO pins for UART function and start the UART with defaults (9600 8N1).
/// Called once from `peripheral_manager::open_uart()`.
pub fn init(uart_id: u8) {
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Ensure IO_BANK0 and PADS_BANK0 are out of reset (idempotent)
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}

    // Release the appropriate UART block from reset
    match uart_id {
        0 => {
            p.RESETS.reset().modify(|_, w| w.uart0().clear_bit());
            while p.RESETS.reset_done().read().uart0().bit_is_clear() {}
        }
        _ => {
            p.RESETS.reset().modify(|_, w| w.uart1().clear_bit());
            while p.RESETS.reset_done().read().uart1().bit_is_clear() {}
        }
    }

    // Route GPIO pins to UART function (function select 2).
    // Default pin assignments:
    //   UART0 → TX=GP0, RX=GP1
    //   UART1 → TX=GP4, RX=GP5
    let (tx_pin, rx_pin): (usize, usize) = match uart_id {
        0 => (0, 1),
        _ => (4, 5),
    };
    for pin in [tx_pin, rx_pin] {
        p.IO_BANK0
            .gpio(pin)
            .gpio_ctrl()
            .write(|w| unsafe { w.funcsel().bits(2) }); // 2 = UART
        p.PADS_BANK0
            .gpio(pin)
            .write(|w| w.ie().set_bit().od().clear_bit()); // enable input buffer
    }

    // Apply default configuration: 9600 8N1, no flow control
    reconfigure(uart_id, 9600, 8, 0, 1, 0);
}

// -------------------------------------------------------------------
// Native method handlers (called from native_handler.rs)
// -------------------------------------------------------------------

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
    reconfigure(id, rate, data_size, parity, stop_bits, hw_flow);
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
    reconfigure(id, baudrate, size, parity, stop_bits, hw_flow);
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
    reconfigure(id, baudrate, data_size, mode, stop_bits, hw_flow);
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
    reconfigure(id, baudrate, data_size, parity, bits, hw_flow);
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
    reconfigure(id, baudrate, data_size, parity, stop_bits, mode);
    Ok(None)
}

/// Blocking write of a single byte. Returns `Some(Int(1))` on success.
pub fn write_byte_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let uart_id = extract_uart_id(args, objects)?;
    let byte = match args.get(1) {
        Some(Value::Int(v)) => *v as u8,
        _ => return Err(JvmError::InvalidReference),
    };
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    match uart_id {
        0 => {
            // Wait until TX FIFO has space
            while p.UART0.uartfr().read().txff().bit_is_set() {}
            p.UART0.uartdr().write(|w| unsafe { w.data().bits(byte) });
        }
        _ => {
            while p.UART1.uartfr().read().txff().bit_is_set() {}
            p.UART1.uartdr().write(|w| unsafe { w.data().bits(byte) });
        }
    }
    Ok(Some(Value::Int(1)))
}

/// Non-blocking read of a single byte. Returns `Some(Int(-1))` if RX FIFO is empty.
pub fn read_byte_native(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let uart_id = extract_uart_id(args, objects)?;
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    let byte = match uart_id {
        0 => {
            if p.UART0.uartfr().read().rxfe().bit_is_set() {
                -1i32
            } else {
                p.UART0.uartdr().read().data().bits() as i32
            }
        }
        _ => {
            if p.UART1.uartfr().read().rxfe().bit_is_set() {
                -1i32
            } else {
                p.UART1.uartdr().read().data().bits() as i32
            }
        }
    };
    Ok(Some(Value::Int(byte)))
}
