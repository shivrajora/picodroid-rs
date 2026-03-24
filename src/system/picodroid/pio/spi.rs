use pico_jvm::{
    array_heap::ArrayHeap,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

// CLK_PERI defaults to system clock: 125 MHz on RP2040, 150 MHz on RP2350
#[cfg(feature = "chip-rp2040")]
const PCLK_HZ: u32 = 125_000_000;
#[cfg(feature = "chip-rp2350")]
const PCLK_HZ: u32 = 150_000_000;

// -------------------------------------------------------------------
// Object field layout for picodroid/pio/SpiDevice in ObjectHeap:
//   field 0: spi_id      (Int: 0=SPI0, 1=SPI1)
//   field 1: frequency_hz (Int, default 1_000_000)
//   field 2: mode         (Int, default 0 = MODE_0: CPOL=0, CPHA=0)
// -------------------------------------------------------------------

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

// Compute SSPCPSR (prescale divisor) and SCR (clock rate scaler).
//   f_spi = PCLK / (CPSDVSR * (1 + SCR))
//   We fix CPSDVSR = 2 (minimum even value) and solve for SCR.
fn clock_divisors(freq_hz: u32) -> (u8, u8) {
    let scr = (PCLK_HZ / (2 * freq_hz.max(1))).saturating_sub(1).min(255) as u8;
    (2u8, scr)
}

// Apply complete SPI config (disable → reprogram → re-enable).
// Uses a macro to handle distinct SPI0/SPI1 PAC types without trait objects.
macro_rules! apply_config {
    ($spi:expr, $freq_hz:expr, $mode:expr) => {{
        // 1. Disable the SSP controller before reconfiguring
        $spi.sspcr1().write(|w| unsafe { w.bits(0) });

        // 2. Clock prescale divisor (must be even, min 2)
        let (cpsdvsr, scr) = clock_divisors($freq_hz);
        $spi.sspcpsr()
            .write(|w| unsafe { w.cpsdvsr().bits(cpsdvsr) });

        // 3. SSPCR0: DSS=8-bit (0b0111), FRF=Motorola SPI (0b00),
        //    SPO=CPOL (mode bit 1), SPH=CPHA (mode bit 0), SCR in bits [15:8]
        let spo: u32 = (($mode >> 1) & 1) as u32;
        let sph: u32 = ($mode & 1) as u32;
        let cr0: u32 = ((scr as u32) << 8) | (sph << 7) | (spo << 6) | 0b0111;
        $spi.sspcr0().write(|w| unsafe { w.bits(cr0) });

        // 4. Re-enable: SSE=1, MS=0 (master mode)
        $spi.sspcr1().write(|w| w.sse().set_bit().ms().clear_bit());
    }};
}

fn reconfigure(spi_id: u8, freq_hz: u32, mode: u32) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    match spi_id {
        0 => apply_config!(&p.SPI0, freq_hz, mode),
        _ => apply_config!(&p.SPI1, freq_hz, mode),
    }
}

/// Configure GPIO pins for SPI function and start the controller at 1 MHz, MODE_0.
/// Called once from `peripheral_manager::open_spi()`.
pub fn init(spi_id: u8) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Ensure IO_BANK0 and PADS_BANK0 are out of reset (idempotent)
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}

    // Release the appropriate SPI block from reset
    match spi_id {
        0 => {
            p.RESETS.reset().modify(|_, w| w.spi0().clear_bit());
            while p.RESETS.reset_done().read().spi0().bit_is_clear() {}
        }
        _ => {
            p.RESETS.reset().modify(|_, w| w.spi1().clear_bit());
            while p.RESETS.reset_done().read().spi1().bit_is_clear() {}
        }
    }

    // Route GPIO pins to SPI function (function select 1).
    // Default pin assignments (3-wire, CS managed separately via Gpio):
    //   SPI0 → SCK=GP2, MOSI(TX)=GP3, MISO(RX)=GP0
    //   SPI1 → SCK=GP10, MOSI(TX)=GP11, MISO(RX)=GP8
    let (sck, mosi, miso): (usize, usize, usize) = match spi_id {
        0 => (2, 3, 0),
        _ => (10, 11, 8),
    };
    for pin in [sck, mosi, miso] {
        p.IO_BANK0
            .gpio(pin)
            .gpio_ctrl()
            .write(|w| unsafe { w.funcsel().bits(1) }); // 1 = SPI
        p.PADS_BANK0
            .gpio(pin)
            .write(|w| w.ie().set_bit().od().clear_bit());
    }

    // Apply default configuration: 1 MHz, MODE_0
    reconfigure(spi_id, 1_000_000, 0);
}

// -------------------------------------------------------------------
// Native method handlers
// -------------------------------------------------------------------

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
    reconfigure(id, hz, mode);
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
    reconfigure(id, hz, mode);
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

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Interleaved TX+RX: one byte at a time to prevent RX FIFO overflow
    macro_rules! do_transfer {
        ($spi:expr) => {{
            for i in 0..len {
                let byte = arrays.load(tx_idx, i).unwrap_or(0) as u8;
                while $spi.sspsr().read().tnf().bit_is_clear() {}
                $spi.sspdr()
                    .write(|w| unsafe { w.data().bits(byte as u16) });
                while $spi.sspsr().read().rne().bit_is_clear() {}
                let received = $spi.sspdr().read().data().bits() as i32;
                arrays.store(rx_idx, i, received);
            }
            while $spi.sspsr().read().bsy().bit_is_set() {}
        }};
    }

    match spi_id {
        0 => do_transfer!(&p.SPI0),
        _ => do_transfer!(&p.SPI1),
    }
    Ok(Some(Value::Int(len as i32)))
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

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    macro_rules! do_write {
        ($spi:expr) => {{
            for i in 0..len {
                let byte = arrays.load(data_idx, i).unwrap_or(0) as u8;
                while $spi.sspsr().read().tnf().bit_is_clear() {}
                $spi.sspdr()
                    .write(|w| unsafe { w.data().bits(byte as u16) });
                while $spi.sspsr().read().rne().bit_is_clear() {}
                let _ = $spi.sspdr().read(); // drain RX FIFO
            }
            while $spi.sspsr().read().bsy().bit_is_set() {}
        }};
    }

    match spi_id {
        0 => do_write!(&p.SPI0),
        _ => do_write!(&p.SPI1),
    }
    Ok(Some(Value::Int(len as i32)))
}
