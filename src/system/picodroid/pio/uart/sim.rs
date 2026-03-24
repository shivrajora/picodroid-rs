pub(super) fn init(_uart_id: u8) {}

pub(super) fn reconfigure(
    _uart_id: u8,
    _baudrate: i32,
    _data_size: i32,
    _parity: i32,
    _stop_bits: i32,
    _hw_flow: i32,
) {
}

/// Blocking write of a single byte — outputs to stdout.
pub(super) fn write_byte(_uart_id: u8, byte: u8) {
    print!("{}", byte as char);
}

/// Non-blocking read — not supported in sim; returns -1 (empty).
pub(super) fn read_byte(_uart_id: u8) -> i32 {
    -1
}
