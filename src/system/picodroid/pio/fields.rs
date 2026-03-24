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
