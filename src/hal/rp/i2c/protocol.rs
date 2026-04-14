//! Pure I2C logic: register-bit constants, SCL timing math, transfer state.
//!
//! No `rp-pico`/`rp235x-hal`/FreeRTOS deps — host-compilable and unit-testable.

// IC_CON bit masks
pub const IC_CON_MASTER_MODE: u32 = 1 << 0;
pub const IC_CON_SPEED_STD: u32 = 1 << 1; // SPEED=01 (standard, 100 kHz)
pub const IC_CON_SPEED_FAST: u32 = 1 << 2; // SPEED=10 (fast, 400 kHz)
pub const IC_CON_RESTART_EN: u32 = 1 << 5;
pub const IC_CON_SLAVE_DISABLE: u32 = 1 << 6;

// IC_DATA_CMD bit masks
pub const IC_DATA_CMD_READ: u32 = 1 << 8;
pub const IC_DATA_CMD_STOP: u32 = 1 << 9;

// IC_INTR_MASK bit positions
pub const INTR_RX_FULL: u32 = 1 << 2;
pub const INTR_TX_EMPTY: u32 = 1 << 4;
pub const INTR_TX_ABRT: u32 = 1 << 6;
pub const INTR_STOP_DET: u32 = 1 << 9;

/// Maximum bytes per interrupt-driven I2C transaction.
pub const MAX_XFER_LEN: usize = 64;

#[derive(Clone, Copy, PartialEq)]
pub enum I2cOp {
    Idle,
    Write,
    Read,
}

pub struct I2cXferState {
    pub op: I2cOp,
    pub buf: [u8; MAX_XFER_LEN],
    pub len: usize,
    pub tx_idx: usize, // write: next byte to push; read: next read cmd to issue
    pub rx_idx: usize, // read: next byte to store
    pub result: i32,   // set by ISR: len on success, -1 on abort
}

impl I2cXferState {
    pub const fn new() -> Self {
        Self {
            op: I2cOp::Idle,
            buf: [0; MAX_XFER_LEN],
            len: 0,
            tx_idx: 0,
            rx_idx: 0,
            result: 0,
        }
    }
}

/// Compute SCL high/low counts for the given peripheral clock and bus speed.
/// Duty cycle is 40% high / 60% low.
pub fn scl_counts(pclk_hz: u32, speed_hz: u32) -> (u16, u16) {
    let period = pclk_hz / speed_hz.max(1);
    let lcnt = (period * 3 / 5) as u16;
    let hcnt = (period - lcnt as u32) as u16;
    (hcnt, lcnt)
}

/// Build the IC_CON register value for the requested bus speed.
pub fn ic_con_for_speed(speed_hz: u32) -> u32 {
    let speed_bits = if speed_hz <= 100_000 {
        IC_CON_SPEED_STD
    } else {
        IC_CON_SPEED_FAST
    };
    IC_CON_MASTER_MODE | speed_bits | IC_CON_RESTART_EN | IC_CON_SLAVE_DISABLE
}
