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
// Object field layout for picodroid/pio/I2cDevice in ObjectHeap:
//   field 0: i2c_id   (Int: 0=I2C0, 1=I2C1)
//   field 1: speed_hz (Int, default 100_000)
// -------------------------------------------------------------------

// IC_CON bit masks
const IC_CON_MASTER_MODE: u32 = 1 << 0;
const IC_CON_SPEED_STD: u32 = 1 << 1; // SPEED=01 (standard, 100 kHz)
const IC_CON_SPEED_FAST: u32 = 1 << 2; // SPEED=10 (fast, 400 kHz)
const IC_CON_RESTART_EN: u32 = 1 << 5;
const IC_CON_SLAVE_DISABLE: u32 = 1 << 6;

// IC_DATA_CMD bit masks
const IC_DATA_CMD_READ: u32 = 1 << 8;
const IC_DATA_CMD_STOP: u32 = 1 << 9;

fn extract_obj_idx(args: &[Value]) -> Result<u16, JvmError> {
    match args.first() {
        Some(Value::ObjectRef(idx)) => Ok(*idx),
        _ => Err(JvmError::InvalidReference),
    }
}

fn extract_i2c_id(args: &[Value], objects: &ObjectHeap) -> Result<u8, JvmError> {
    let idx = extract_obj_idx(args)?;
    match objects.get_field(idx, 0) {
        Some(Value::Int(id)) => Ok(id as u8),
        _ => Err(JvmError::InvalidReference),
    }
}

// Compute SCL high/low counts from desired speed.
// period = PCLK_HZ / speed_hz; lcnt = 60%, hcnt = 40%.
fn scl_counts(speed_hz: u32) -> (u16, u16) {
    let period = PCLK_HZ / speed_hz.max(1);
    let lcnt = (period * 3 / 5) as u16;
    let hcnt = (period - lcnt as u32) as u16;
    (hcnt, lcnt)
}

fn ic_con_for_speed(speed_hz: u32) -> u32 {
    let speed_bits = if speed_hz <= 100_000 {
        IC_CON_SPEED_STD
    } else {
        IC_CON_SPEED_FAST
    };
    IC_CON_MASTER_MODE | speed_bits | IC_CON_RESTART_EN | IC_CON_SLAVE_DISABLE
}

// Apply speed configuration to the hardware (disable → reprogram → re-enable).
// Uses a macro to handle the two distinct I2C PAC types without trait objects.
macro_rules! apply_speed {
    ($i2c:expr, $speed_hz:expr) => {{
        // Disable controller before reconfiguring
        $i2c.ic_enable().write(|w| unsafe { w.bits(0) });
        while $i2c.ic_enable_status().read().ic_en().bit_is_set() {}

        // IC_CON: master mode, speed, restart enabled, slave disabled
        let con = ic_con_for_speed($speed_hz);
        $i2c.ic_con().write(|w| unsafe { w.bits(con) });

        // SCL counts — program both SS and FS registers so setSpeed() can
        // switch between modes without re-init.
        let (hcnt, lcnt) = scl_counts($speed_hz);
        $i2c.ic_ss_scl_hcnt()
            .write(|w| unsafe { w.ic_ss_scl_hcnt().bits(hcnt) });
        $i2c.ic_ss_scl_lcnt()
            .write(|w| unsafe { w.ic_ss_scl_lcnt().bits(lcnt) });
        $i2c.ic_fs_scl_hcnt()
            .write(|w| unsafe { w.ic_fs_scl_hcnt().bits(hcnt) });
        $i2c.ic_fs_scl_lcnt()
            .write(|w| unsafe { w.ic_fs_scl_lcnt().bits(lcnt) });

        // Re-enable
        $i2c.ic_enable().write(|w| unsafe { w.bits(1) });
    }};
}

fn reconfigure(i2c_id: u8, speed_hz: u32) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    match i2c_id {
        0 => apply_speed!(&p.I2C0, speed_hz),
        _ => apply_speed!(&p.I2C1, speed_hz),
    }
}

/// Configure GPIO pins for I2C function and start the controller at 100 kHz.
/// Called once from `peripheral_manager::open_i2c()`.
pub fn init(i2c_id: u8) {
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

    // Release the appropriate I2C block from reset
    match i2c_id {
        0 => {
            p.RESETS.reset().modify(|_, w| w.i2c0().clear_bit());
            while p.RESETS.reset_done().read().i2c0().bit_is_clear() {}
        }
        _ => {
            p.RESETS.reset().modify(|_, w| w.i2c1().clear_bit());
            while p.RESETS.reset_done().read().i2c1().bit_is_clear() {}
        }
    }

    // Route GPIO pins to I2C function (function select 3).
    // Default pin assignments:
    //   I2C0 → SDA=GP4, SCL=GP5
    //   I2C1 → SDA=GP2, SCL=GP3
    let (sda_pin, scl_pin): (usize, usize) = match i2c_id {
        0 => (4, 5),
        _ => (2, 3),
    };
    for pin in [sda_pin, scl_pin] {
        p.IO_BANK0
            .gpio(pin)
            .gpio_ctrl()
            .write(|w| unsafe { w.funcsel().bits(3) }); // 3 = I2C
                                                        // Enable input, pull-up (open-drain bus), Schmitt trigger
        p.PADS_BANK0.gpio(pin).write(|w| {
            w.ie()
                .set_bit()
                .od()
                .clear_bit()
                .pue()
                .set_bit()
                .schmitt()
                .set_bit()
        });
    }

    // Apply default configuration: 100 kHz standard speed
    reconfigure(i2c_id, 100_000);
}

// -------------------------------------------------------------------
// Native method handlers
// -------------------------------------------------------------------

pub fn set_speed_native(
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
    let id = match objects.get_field(idx, 0) {
        Some(Value::Int(v)) => v as u8,
        _ => return Err(JvmError::InvalidReference),
    };
    reconfigure(id, hz);
    Ok(None)
}

/// Blocking write. args: [this, Int(address), ArrayRef(data), Int(len)]
/// Returns Int(len) on success, Int(-1) on NACK/abort.
pub fn write_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let i2c_id = extract_i2c_id(args, objects)?;
    let address = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    let data_idx = match args.get(2) {
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

    macro_rules! do_write {
        ($i2c:expr) => {{
            // Set target address
            $i2c.ic_tar()
                .write(|w| unsafe { w.ic_tar().bits(address as u16) });

            if len == 0 {
                // Zero-byte write: send address + STOP to probe for ACK
                while $i2c.ic_status().read().tfnf().bit_is_clear() {}
                $i2c.ic_data_cmd()
                    .write(|w| unsafe { w.bits(IC_DATA_CMD_STOP) });
            } else {
                for i in 0..len {
                    let byte = arrays.load(data_idx, i).unwrap_or(0) as u8;
                    let stop = if i == len - 1 { IC_DATA_CMD_STOP } else { 0 };
                    while $i2c.ic_status().read().tfnf().bit_is_clear() {}
                    $i2c.ic_data_cmd()
                        .write(|w| unsafe { w.bits(byte as u32 | stop) });
                }
            }

            // Wait for TX FIFO to drain and bus to go idle
            while $i2c.ic_status().read().tfe().bit_is_clear() {}
            while $i2c.ic_status().read().mst_activity().bit_is_set() {}

            // Check for abort (NACK or arbitration loss)
            if $i2c.ic_raw_intr_stat().read().tx_abrt().bit_is_set() {
                let _ = $i2c.ic_clr_tx_abrt().read();
                -1i32
            } else {
                len as i32
            }
        }};
    }

    let result = match i2c_id {
        0 => do_write!(&p.I2C0),
        _ => do_write!(&p.I2C1),
    };
    Ok(Some(Value::Int(result)))
}

/// Blocking read. args: [this, Int(address), ArrayRef(buf), Int(len)]
/// Returns Int(len) on success, Int(-1) on NACK/abort.
pub fn read_native(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let i2c_id = extract_i2c_id(args, objects)?;
    let address = match args.get(1) {
        Some(Value::Int(v)) => *v as u32,
        _ => return Err(JvmError::InvalidReference),
    };
    let buf_idx = match args.get(2) {
        Some(Value::ArrayRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let len = match args.get(3) {
        Some(Value::Int(v)) => *v as usize,
        _ => return Err(JvmError::InvalidReference),
    };

    if len == 0 {
        return Ok(Some(Value::Int(0)));
    }

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    macro_rules! do_read {
        ($i2c:expr) => {{
            // Set target address
            $i2c.ic_tar()
                .write(|w| unsafe { w.ic_tar().bits(address as u16) });

            // Interleaved: send one read command, collect one RX byte — avoids overflow
            for i in 0..len {
                let stop = if i == len - 1 { IC_DATA_CMD_STOP } else { 0 };
                while $i2c.ic_status().read().tfnf().bit_is_clear() {}
                $i2c.ic_data_cmd()
                    .write(|w| unsafe { w.bits(IC_DATA_CMD_READ | stop) });
                while $i2c.ic_status().read().rfne().bit_is_clear() {}
                let byte = $i2c.ic_data_cmd().read().dat().bits() as i32;
                arrays.store(buf_idx, i, byte);
            }

            // Check for abort
            if $i2c.ic_raw_intr_stat().read().tx_abrt().bit_is_set() {
                let _ = $i2c.ic_clr_tx_abrt().read();
                -1i32
            } else {
                len as i32
            }
        }};
    }

    let result = match i2c_id {
        0 => do_read!(&p.I2C0),
        _ => do_read!(&p.I2C1),
    };
    Ok(Some(Value::Int(result)))
}
