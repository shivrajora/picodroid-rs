/// Errors that can occur during a transport read operation.
pub enum ReadError {
    /// The transport timed out waiting for data.
    Timeout,
}

/// Errors reported by install orchestration to the transport.
pub enum InstallError {
    /// PAPK size is zero.
    EmptyPayload,
    /// PAPK size exceeds flash capacity.
    TooLarge,
    /// Core 0 did not park within the timeout window.
    ParkTimeout,
    /// Data stream timed out mid-transfer.
    StreamTimeout,
    /// Flash page write failed.
    FlashWriteFailed,
    /// CRC mismatch after all data received.
    CrcMismatch,
}

/// A data source and status reporter for PAPK install operations.
///
/// Implementors provide byte-level reads (with timeout semantics) and status
/// reporting.  The install orchestrator calls these methods without knowing
/// what transport is underneath (UART, BLE, OTA, SPI, …).
pub trait InstallTransport {
    /// Read exactly one byte from the transport.
    ///
    /// Returns `Err(ReadError::Timeout)` if no byte arrives within the
    /// transport's configured timeout window.
    fn read_byte(&mut self) -> Result<u8, ReadError>;

    /// Read a `u32` in little-endian byte order.
    ///
    /// The default implementation calls [`read_byte`](Self::read_byte) four
    /// times.  Transports with bulk-read capability may override this.
    fn read_u32_le(&mut self) -> Result<u32, ReadError> {
        let b0 = self.read_byte()? as u32;
        let b1 = self.read_byte()? as u32;
        let b2 = self.read_byte()? as u32;
        let b3 = self.read_byte()? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    }

    /// Report that the device has erased flash and is ready to receive data.
    ///
    /// Transports with a back-channel send an acknowledgment; transports
    /// without one may no-op.
    fn report_ready(&mut self);

    /// Report successful install completion.
    ///
    /// The transport must ensure the response is fully transmitted before this
    /// method returns (e.g. drain the TX FIFO).
    fn report_success(&mut self);

    /// Report an install error.
    ///
    /// Transports with a back-channel send the error to the host; transports
    /// without one may log or no-op.
    fn report_error(&mut self, error: InstallError);
}
