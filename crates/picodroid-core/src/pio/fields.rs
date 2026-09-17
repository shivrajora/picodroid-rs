// SPDX-License-Identifier: GPL-3.0-only
//! Field slot indices of the `picodroid.pio` peripheral objects. Slot 0 is
//! the one field each Java class declares (checked against the class files
//! by `native_field_tables_tests`); the rest are native-side state the
//! peripheral manager keeps on the object, numbered by slot — a `double`
//! takes two.
pub mod gpio {
    pub const PIN: usize = 0;
}

pub mod uart {
    pub const UART_ID: usize = 0;
    pub const BAUDRATE: usize = 1;
    pub const DATA_SIZE: usize = 2;
    pub const PARITY: usize = 3;
    pub const STOP_BITS: usize = 4;
    pub const HW_FLOW: usize = 5;
}

pub mod i2c {
    pub const I2C_ID: usize = 0;
    pub const SPEED_HZ: usize = 1;
}

pub mod spi {
    pub const SPI_ID: usize = 0;
    pub const FREQUENCY_HZ: usize = 1;
    pub const MODE: usize = 2;
}

pub mod pwm {
    pub const PIN: usize = 0;
    /// `double`s, two slots each: 1-2 and 3-4. Native-side state past the
    /// one field `Pwm.java` declares.
    pub const FREQUENCY_HZ: usize = 1;
    pub const DUTY_CYCLE: usize = 3;
    pub const ENABLED: usize = 5;
}

pub mod adc {
    pub const PIN: usize = 0;
}
