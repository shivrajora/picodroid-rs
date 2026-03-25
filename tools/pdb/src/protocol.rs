use std::io::{self, Read, Write};
use std::time::Duration;

pub const FRAME_MAGIC: &[u8; 4] = b"PDBP";
pub const CMD_PING: u8 = 0x00;
pub const CMD_INSTALL: u8 = 0x01;
pub const STATUS_OK: u8 = 0x00;
pub const STATUS_ERR: u8 = 0xFF;
pub const STATUS_TOO_LARGE: u8 = 0xFE;
pub const STATUS_CRC_FAIL: u8 = 0xFD;

/// CRC32 over [cmd byte][4-byte len LE][payload] — mirrors firmware crc32_frame.
pub fn crc32_frame(cmd: u8, len: u32, payload: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(&[cmd]);
    h.update(&len.to_le_bytes());
    h.update(payload);
    h.finalize()
}

/// Send a PDBP request frame.
pub fn send_frame(port: &mut dyn Write, cmd: u8, payload: &[u8]) -> io::Result<()> {
    let len = payload.len() as u32;
    let crc = crc32_frame(cmd, len, payload);
    port.write_all(FRAME_MAGIC)?;
    port.write_all(&[cmd])?;
    port.write_all(&len.to_le_bytes())?;
    port.write_all(payload)?;
    port.write_all(&crc.to_le_bytes())?;
    port.flush()
}

/// Receive and parse a PDBP response frame.
///
/// Returns `(status, payload_bytes)`.
pub fn recv_response(port: &mut dyn Read) -> io::Result<(u8, Vec<u8>)> {
    // Read magic
    let mut magic = [0u8; 4];
    port.read_exact(&mut magic)?;
    if &magic != FRAME_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bad magic: expected {:?}, got {:?}", FRAME_MAGIC, magic),
        ));
    }

    let mut status_buf = [0u8; 1];
    port.read_exact(&mut status_buf)?;
    let status = status_buf[0];

    let mut len_buf = [0u8; 4];
    port.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut payload = vec![0u8; len];
    port.read_exact(&mut payload)?;

    Ok((status, payload))
}

/// Status byte to human-readable string.
pub fn status_str(s: u8) -> &'static str {
    match s {
        STATUS_OK => "OK",
        STATUS_ERR => "ERR",
        STATUS_TOO_LARGE => "TOO_LARGE",
        STATUS_CRC_FAIL => "CRC_FAIL",
        _ => "UNKNOWN",
    }
}

/// Timeout used when waiting for a PING response after install.
pub const POLL_TIMEOUT: Duration = Duration::from_millis(200);
/// Number of PING polls after install before giving up.
pub const POLL_ATTEMPTS: u32 = 25; // 5 seconds total
