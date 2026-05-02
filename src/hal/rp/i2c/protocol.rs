// SPDX-License-Identifier: GPL-3.0-only
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── Bit mask uniqueness ───────────────────────────────────────────────

    #[test]
    fn ic_con_speed_bits_are_distinct() {
        assert_ne!(IC_CON_SPEED_STD, IC_CON_SPEED_FAST);
        assert_eq!(IC_CON_SPEED_STD & IC_CON_SPEED_FAST, 0);
    }

    #[test]
    fn ic_data_cmd_read_and_stop_distinct() {
        assert_eq!(IC_DATA_CMD_READ & IC_DATA_CMD_STOP, 0);
    }

    #[test]
    fn intr_bits_are_distinct() {
        let bits = [INTR_RX_FULL, INTR_TX_EMPTY, INTR_TX_ABRT, INTR_STOP_DET];
        for (i, a) in bits.iter().enumerate() {
            for b in &bits[i + 1..] {
                assert_eq!(a & b, 0);
            }
        }
    }

    // ── I2cXferState::new ─────────────────────────────────────────────────

    #[test]
    fn new_state_is_idle() {
        let s = I2cXferState::new();
        assert!(s.op == I2cOp::Idle);
        assert_eq!(s.len, 0);
        assert_eq!(s.tx_idx, 0);
        assert_eq!(s.rx_idx, 0);
        assert_eq!(s.result, 0);
    }

    #[test]
    fn new_state_buf_is_zeroed_and_sized() {
        let s = I2cXferState::new();
        assert_eq!(s.buf.len(), MAX_XFER_LEN);
        assert!(s.buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn i2c_op_variants_distinct() {
        assert!(I2cOp::Idle != I2cOp::Write);
        assert!(I2cOp::Write != I2cOp::Read);
        assert!(I2cOp::Idle != I2cOp::Read);
    }

    #[test]
    fn i2c_op_copy() {
        let a = I2cOp::Read;
        let b = a;
        assert!(a == b);
    }

    // ── scl_counts ────────────────────────────────────────────────────────

    #[test]
    fn scl_counts_split_is_60_40() {
        let pclk = 125_000_000;
        let (hcnt, lcnt) = scl_counts(pclk, 100_000);
        let period = pclk / 100_000;
        assert_eq!(lcnt as u32, period * 3 / 5);
        assert_eq!(hcnt as u32 + lcnt as u32, period);
    }

    #[test]
    fn scl_counts_100khz_at_125mhz_pclk() {
        let (hcnt, lcnt) = scl_counts(125_000_000, 100_000);
        // period = 1250, lcnt = 750, hcnt = 500
        assert_eq!(lcnt, 750);
        assert_eq!(hcnt, 500);
    }

    #[test]
    fn scl_counts_400khz_at_125mhz_pclk() {
        let (hcnt, lcnt) = scl_counts(125_000_000, 400_000);
        // period = 312, lcnt = 187, hcnt = 125
        assert_eq!(lcnt, 187);
        assert_eq!(hcnt, 125);
    }

    #[test]
    fn scl_counts_1mhz_at_125mhz_pclk() {
        let (hcnt, lcnt) = scl_counts(125_000_000, 1_000_000);
        // period = 125, lcnt = 75, hcnt = 50
        assert_eq!(lcnt, 75);
        assert_eq!(hcnt, 50);
    }

    #[test]
    fn scl_counts_higher_speed_smaller_period() {
        let (h100k, l100k) = scl_counts(125_000_000, 100_000);
        let (h400k, l400k) = scl_counts(125_000_000, 400_000);
        assert!(h400k < h100k);
        assert!(l400k < l100k);
    }

    #[test]
    fn scl_counts_speed_zero_does_not_divide_by_zero() {
        let (hcnt, lcnt) = scl_counts(125_000_000, 0);
        // speed.max(1) ⇒ period = pclk. lcnt/hcnt saturate to u16.
        assert!(hcnt > 0 || lcnt > 0);
    }

    // ── ic_con_for_speed ──────────────────────────────────────────────────

    #[test]
    fn ic_con_100khz_uses_std_speed() {
        let v = ic_con_for_speed(100_000);
        assert_eq!(v & IC_CON_SPEED_STD, IC_CON_SPEED_STD);
        assert_eq!(v & IC_CON_SPEED_FAST, 0);
    }

    #[test]
    fn ic_con_400khz_uses_fast_speed() {
        let v = ic_con_for_speed(400_000);
        assert_eq!(v & IC_CON_SPEED_FAST, IC_CON_SPEED_FAST);
        assert_eq!(v & IC_CON_SPEED_STD, 0);
    }

    #[test]
    fn ic_con_1mhz_uses_fast_speed() {
        let v = ic_con_for_speed(1_000_000);
        assert_eq!(v & IC_CON_SPEED_FAST, IC_CON_SPEED_FAST);
    }

    #[test]
    fn ic_con_boundary_100khz_std_inclusive() {
        // Exactly 100 kHz stays STD.
        let v = ic_con_for_speed(100_000);
        assert_eq!(v & IC_CON_SPEED_STD, IC_CON_SPEED_STD);
    }

    #[test]
    fn ic_con_always_master_restart_slave_disabled() {
        for speed in [100_000u32, 400_000, 1_000_000] {
            let v = ic_con_for_speed(speed);
            assert_eq!(v & IC_CON_MASTER_MODE, IC_CON_MASTER_MODE);
            assert_eq!(v & IC_CON_RESTART_EN, IC_CON_RESTART_EN);
            assert_eq!(v & IC_CON_SLAVE_DISABLE, IC_CON_SLAVE_DISABLE);
        }
    }

    #[test]
    fn ic_con_has_exactly_one_speed_bit_set() {
        for speed in [1u32, 100_000, 400_000, 1_000_000] {
            let v = ic_con_for_speed(speed);
            let speed_mask = IC_CON_SPEED_STD | IC_CON_SPEED_FAST;
            let set = (v & speed_mask).count_ones();
            assert_eq!(set, 1, "speed={}", speed);
        }
    }
}
