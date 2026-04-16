use std::io::{self, Read, Write};
use std::time::Duration;

pub const FRAME_MAGIC: &[u8; 4] = b"PDBP";
pub const CMD_PING: u8 = 0x00;
pub const CMD_INSTALL: u8 = 0x01;
pub const CMD_SYSMON: u8 = 0x02;
pub const STATUS_OK: u8 = 0x00;
pub const STATUS_READY: u8 = 0x01; // device erased flash, ready to receive data stream
pub const STATUS_ERR: u8 = 0xFF;
pub const STATUS_TOO_LARGE: u8 = 0xFE;
pub const STATUS_CRC_FAIL: u8 = 0xFD;
/// Device refused the install: PAPK's framework-map-version is incompatible
/// with the running firmware. Returned in install Phase A *before* any
/// flash erase, so the existing PAPK on-device is unaffected.
pub const STATUS_INCOMPAT: u8 = 0xFC;

/// Number of PAPK bytes the host sends inline immediately after the install
/// header (Phase A) — must match the device's `INSTALL_PEEK_BYTES`
/// constant. Lets the device peek the manifest section before erasing
/// flash. The remaining `papk_len - INSTALL_PEEK_BYTES` bytes (if any)
/// stream after STATUS_READY.
pub const INSTALL_PEEK_BYTES: usize = 512;

/// CRC32 over [cmd byte][4-byte len LE][payload] — mirrors firmware crc32_frame.
pub fn crc32_frame(cmd: u8, len: u32, payload: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(&[cmd]);
    h.update(&len.to_le_bytes());
    h.update(payload);
    h.finalize()
}

/// Send a standard PDBP request frame (magic + cmd + len + payload + CRC32).
/// Used for CMD_PING and similar framed commands.
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

/// Send the CMD_INSTALL Phase A header: [PDBP][CMD_INSTALL][papk_len: u32 LE].
///
/// No payload and no CRC — the device erases flash immediately after reading
/// this 9-byte header, then responds with STATUS_READY.
pub fn send_install_header(port: &mut dyn Write, papk_len: u32) -> io::Result<()> {
    port.write_all(FRAME_MAGIC)?;
    port.write_all(&[CMD_INSTALL])?;
    port.write_all(&papk_len.to_le_bytes())?;
    port.flush()
}

/// Send the CMD_INSTALL Phase B data stream: `tail` bytes followed by CRC32.
///
/// `tail` is the portion of the PAPK that hasn't already been sent inline
/// during Phase A's peek payload (typically `papk[INSTALL_PEEK_BYTES..]`).
/// `full_papk` is the **complete** PAPK, used only for the CRC32 — which
/// must cover every byte in [CMD_INSTALL][papk_len_le:4][papk_bytes] to
/// match the device's incremental hasher.
pub fn send_install_data(
    port: &mut dyn Write,
    papk_len: u32,
    full_papk: &[u8],
    tail: &[u8],
) -> io::Result<()> {
    let crc = crc32_frame(CMD_INSTALL, papk_len, full_papk);
    port.write_all(tail)?;
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
        STATUS_READY => "READY",
        STATUS_ERR => "ERR",
        STATUS_TOO_LARGE => "TOO_LARGE",
        STATUS_CRC_FAIL => "CRC_FAIL",
        STATUS_INCOMPAT => "INCOMPAT",
        _ => "UNKNOWN",
    }
}

/// Timeout used when waiting for a PING response after install.
pub const POLL_TIMEOUT: Duration = Duration::from_millis(500);
/// Number of PING polls after install before giving up (20 seconds total).
pub const POLL_ATTEMPTS: u32 = 40;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ── CRC correctness ───────────────────────────────────────────────────────

    /// The device streaming handler feeds the PAPK to the CRC in 256-byte
    /// chunks (page-sized writes).  Verify that the host's single-shot
    /// `crc32_frame` produces the same value so installs never fail with
    /// STATUS_CRC_FAIL due to a hasher mismatch.
    #[test]
    fn crc_single_shot_matches_chunked() {
        let papk = b"PAPK0123456789abcdefghijklmnopqrstuvwxyz".repeat(20); // 800 bytes
        let len = papk.len() as u32;

        // Host: single-shot (what send_install_data computes)
        let single_shot = crc32_frame(CMD_INSTALL, len, &papk);

        // Device: incremental, 256-byte pages (mirrors task.rs streaming handler)
        let mut h = crc32fast::Hasher::new();
        h.update(&[CMD_INSTALL]);
        h.update(&len.to_le_bytes());
        for chunk in papk.chunks(256) {
            h.update(chunk);
        }
        let chunked = h.finalize();

        assert_eq!(single_shot, chunked);
    }

    #[test]
    fn crc_is_deterministic() {
        let a = crc32_frame(CMD_INSTALL, 6, b"foobar");
        let b = crc32_frame(CMD_INSTALL, 6, b"foobar");
        assert_eq!(a, b);
    }

    #[test]
    fn crc_differs_on_different_payload() {
        let a = crc32_frame(CMD_INSTALL, 6, b"foobar");
        let b = crc32_frame(CMD_INSTALL, 6, b"foobaz");
        assert_ne!(a, b);
    }

    #[test]
    fn crc_differs_on_different_cmd() {
        let a = crc32_frame(CMD_PING, 6, b"foobar");
        let b = crc32_frame(CMD_INSTALL, 6, b"foobar");
        assert_ne!(a, b);
    }

    // ── send_install_header ───────────────────────────────────────────────────

    /// Phase A header must be exactly 9 bytes: [PDBP][CMD_INSTALL][len_le:4].
    /// The device reads exactly this many bytes before erasing flash.
    #[test]
    fn install_header_is_9_bytes() {
        let mut buf = Vec::new();
        send_install_header(&mut buf, 0).unwrap();
        assert_eq!(buf.len(), 9);
    }

    #[test]
    fn install_header_structure() {
        let mut buf = Vec::new();
        send_install_header(&mut buf, 0x0001_2345).unwrap();
        assert_eq!(&buf[0..4], FRAME_MAGIC);
        assert_eq!(buf[4], CMD_INSTALL);
        assert_eq!(&buf[5..9], &0x0001_2345u32.to_le_bytes());
    }

    // ── send_install_data ─────────────────────────────────────────────────────

    /// Phase B stream is `tail` bytes followed by CRC32. The CRC must
    /// cover the *full* PAPK (including the peek bytes already sent inline
    /// during Phase A), even though only the tail goes on the wire here.
    #[test]
    fn install_data_ends_with_correct_crc() {
        let papk = b"test papk payload";
        let papk_len = papk.len() as u32;
        let mut buf = Vec::new();
        // Tail = full PAPK (no peek inlined): equivalent to old behavior.
        send_install_data(&mut buf, papk_len, papk, papk).unwrap();

        assert_eq!(buf.len(), papk.len() + 4);
        assert_eq!(&buf[..papk.len()], papk.as_slice());

        let wire_crc = u32::from_le_bytes(buf[papk.len()..].try_into().unwrap());
        assert_eq!(wire_crc, crc32_frame(CMD_INSTALL, papk_len, papk));
    }

    /// When part of the PAPK is sent inline during Phase A (peek), the
    /// Phase B tail is shorter but the CRC still covers the whole PAPK.
    #[test]
    fn install_data_crc_covers_full_papk_when_tail_is_partial() {
        let papk = b"0123456789abcdefghij"; // 20 bytes
        let papk_len = papk.len() as u32;
        let peek_len = 8;
        let tail = &papk[peek_len..]; // 12 bytes

        let mut buf = Vec::new();
        send_install_data(&mut buf, papk_len, papk, tail).unwrap();

        // Wire = tail bytes + CRC.
        assert_eq!(buf.len(), tail.len() + 4);
        assert_eq!(&buf[..tail.len()], tail);

        let wire_crc = u32::from_le_bytes(buf[tail.len()..].try_into().unwrap());
        // CRC must match the full PAPK, not just the tail.
        assert_eq!(wire_crc, crc32_frame(CMD_INSTALL, papk_len, papk));
        assert_ne!(wire_crc, crc32_frame(CMD_INSTALL, papk_len, tail));
    }

    // ── recv_response ─────────────────────────────────────────────────────────

    fn encode_response(status: u8, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(FRAME_MAGIC);
        buf.push(status);
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    #[test]
    fn recv_response_ok_with_payload() {
        let raw = encode_response(STATUS_OK, b"picodroid/2.0\0\x00\x00\x02\x00");
        let (status, payload) = recv_response(&mut Cursor::new(raw)).unwrap();
        assert_eq!(status, STATUS_OK);
        assert_eq!(&payload[..14], b"picodroid/2.0\0");
    }

    #[test]
    fn recv_response_ready_empty_payload() {
        let raw = encode_response(STATUS_READY, b"");
        let (status, payload) = recv_response(&mut Cursor::new(raw)).unwrap();
        assert_eq!(status, STATUS_READY);
        assert!(payload.is_empty());
    }

    #[test]
    fn recv_response_rejects_bad_magic() {
        let mut raw = encode_response(STATUS_OK, b"");
        raw[0] = b'X'; // corrupt magic
        assert!(recv_response(&mut Cursor::new(raw)).is_err());
    }

    #[test]
    fn recv_response_error_carries_message() {
        let raw = encode_response(STATUS_CRC_FAIL, b"");
        let (status, _) = recv_response(&mut Cursor::new(raw)).unwrap();
        assert_eq!(status, STATUS_CRC_FAIL);
    }

    // ── send_frame / recv_response round-trip ─────────────────────────────────

    #[test]
    fn ping_frame_has_correct_structure() {
        let mut buf = Vec::new();
        send_frame(&mut buf, CMD_PING, b"").unwrap();
        // [PDBP:4][cmd:1][len:4][crc:4] = 13 bytes
        assert_eq!(buf.len(), 13);
        assert_eq!(&buf[0..4], FRAME_MAGIC);
        assert_eq!(buf[4], CMD_PING);
        assert_eq!(&buf[5..9], &0u32.to_le_bytes());
        let wire_crc = u32::from_le_bytes(buf[9..13].try_into().unwrap());
        assert_eq!(wire_crc, crc32_frame(CMD_PING, 0, b""));
    }
}
