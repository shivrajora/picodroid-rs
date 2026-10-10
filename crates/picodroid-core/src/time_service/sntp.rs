// SPDX-License-Identifier: GPL-3.0-only
//! SNTP (RFC 4330) on the wire: one 48-byte client request and the server's
//! reply, as bytes. No sockets here, so the codec is tested on the host; the
//! exchange is `task::sync_once`.
//!
//! The request carries a nonce in its transmit-timestamp field, which the
//! server echoes as the originate timestamp: a reply that does not echo it
//! is a stray or a late packet from an earlier attempt and is dropped. The
//! reply's transmit timestamp is the server's time when it answered; adding
//! half the round trip gives the time at the moment the reply arrived, which
//! is what the wall clock is anchored to.

/// NTP port.
pub const PORT: u16 = 123;
/// The packet size, both ways.
pub const PACKET_LEN: usize = 48;

const MODE_CLIENT: u8 = 3;
const MODE_SERVER: u8 = 4;
const VERSION: u8 = 4;
/// "Leap indicator 3": the server's clock is not synchronised.
const LI_UNSYNCHRONISED: u8 = 3;
const ORIGINATE_OFFSET: usize = 24;
const TRANSMIT_OFFSET: usize = 40;

/// Seconds between the NTP era (1900-01-01) and the Unix epoch (1970-01-01).
const SECONDS_1900_TO_1970: i64 = 2_208_988_800;

/// Why a reply was not usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyError {
    /// Fewer than 48 bytes.
    Short,
    /// Not a server-mode packet.
    NotServer,
    /// The server says its own clock is not synchronised (LI = 3, or
    /// stratum 0 "kiss-o'-death" / 16+ "unsynchronised").
    Unsynchronised,
    /// The originate timestamp is not our nonce: a stray or a late reply.
    Nonce,
    /// The transmit timestamp is zero.
    ZeroTime,
}

impl ReplyError {
    /// A short name for the log.
    pub fn name(self) -> &'static str {
        match self {
            ReplyError::Short => "short reply",
            ReplyError::NotServer => "not a server reply",
            ReplyError::Unsynchronised => "server unsynchronised",
            ReplyError::Nonce => "nonce mismatch",
            ReplyError::ZeroTime => "zero transmit time",
        }
    }
}

/// A client request: version 4, mode 3, `nonce` in the transmit field.
pub fn request(nonce: u64) -> [u8; PACKET_LEN] {
    let mut buf = [0u8; PACKET_LEN];
    buf[0] = (VERSION << 3) | MODE_CLIENT;
    buf[TRANSMIT_OFFSET..TRANSMIT_OFFSET + 8].copy_from_slice(&nonce.to_be_bytes());
    buf
}

/// Decode a reply to the request that carried `nonce`, received `rtt_ms`
/// after the request went out: the Unix epoch milliseconds at the moment
/// the reply arrived.
pub fn parse_reply(buf: &[u8], nonce: u64, rtt_ms: i64) -> Result<i64, ReplyError> {
    if buf.len() < PACKET_LEN {
        return Err(ReplyError::Short);
    }
    let li = buf[0] >> 6;
    let mode = buf[0] & 0x7;
    let stratum = buf[1];
    if mode != MODE_SERVER {
        return Err(ReplyError::NotServer);
    }
    if li == LI_UNSYNCHRONISED || stratum == 0 || stratum > 15 {
        return Err(ReplyError::Unsynchronised);
    }
    let originate = u64::from_be_bytes(
        buf[ORIGINATE_OFFSET..ORIGINATE_OFFSET + 8]
            .try_into()
            .map_err(|_| ReplyError::Short)?,
    );
    if originate != nonce {
        return Err(ReplyError::Nonce);
    }
    let transmit_ms = timestamp_ms(&buf[TRANSMIT_OFFSET..TRANSMIT_OFFSET + 8]);
    if transmit_ms == 0 {
        return Err(ReplyError::ZeroTime);
    }
    Ok(transmit_ms + rtt_ms / 2)
}

/// A 64-bit NTP timestamp (seconds since 1900, 32-bit fraction) as Unix
/// epoch milliseconds; 0 for the zero timestamp.
fn timestamp_ms(ts: &[u8]) -> i64 {
    let seconds = u32::from_be_bytes([ts[0], ts[1], ts[2], ts[3]]);
    let fraction = u32::from_be_bytes([ts[4], ts[5], ts[6], ts[7]]);
    if seconds == 0 {
        return 0;
    }
    // Era 0 runs out in 2036; seconds below the 1970 mark then mean era 1.
    let mut seconds = i64::from(seconds);
    if seconds < SECONDS_1900_TO_1970 {
        seconds += 1 << 32;
    }
    let fraction_ms = (i64::from(fraction) * 1000) >> 32;
    (seconds - SECONDS_1900_TO_1970) * 1000 + fraction_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-09 12:00:00 UTC in NTP seconds.
    const T_NTP: u32 = (1_791_540_000u64 + 2_208_988_800) as u32;

    fn reply(nonce: u64, li: u8, mode: u8, stratum: u8, transmit_s: u32, frac: u32) -> Vec<u8> {
        let mut b = vec![0u8; PACKET_LEN];
        b[0] = (li << 6) | (VERSION << 3) | mode;
        b[1] = stratum;
        b[ORIGINATE_OFFSET..ORIGINATE_OFFSET + 8].copy_from_slice(&nonce.to_be_bytes());
        b[TRANSMIT_OFFSET..TRANSMIT_OFFSET + 4].copy_from_slice(&transmit_s.to_be_bytes());
        b[TRANSMIT_OFFSET + 4..TRANSMIT_OFFSET + 8].copy_from_slice(&frac.to_be_bytes());
        b
    }

    #[test]
    fn request_is_v4_client_with_the_nonce_in_transmit() {
        let r = request(0x0102_0304_0506_0708);
        assert_eq!(r[0], 0x23);
        assert_eq!(&r[40..48], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(r[1..40].iter().all(|b| *b == 0));
    }

    #[test]
    fn reply_decodes_to_epoch_millis_plus_half_the_round_trip() {
        let b = reply(7, 0, MODE_SERVER, 2, T_NTP, 1 << 31);
        assert_eq!(parse_reply(&b, 7, 100), Ok(1_791_540_000_000 + 500 + 50));
    }

    #[test]
    fn bad_replies_are_named() {
        assert_eq!(parse_reply(&[0; 10], 7, 0), Err(ReplyError::Short));
        let b = reply(7, 0, MODE_CLIENT, 2, T_NTP, 0);
        assert_eq!(parse_reply(&b, 7, 0), Err(ReplyError::NotServer));
        let b = reply(7, 3, MODE_SERVER, 2, T_NTP, 0);
        assert_eq!(parse_reply(&b, 7, 0), Err(ReplyError::Unsynchronised));
        let b = reply(7, 0, MODE_SERVER, 0, T_NTP, 0);
        assert_eq!(parse_reply(&b, 7, 0), Err(ReplyError::Unsynchronised));
        let b = reply(8, 0, MODE_SERVER, 2, T_NTP, 0);
        assert_eq!(parse_reply(&b, 7, 0), Err(ReplyError::Nonce));
        let b = reply(7, 0, MODE_SERVER, 2, 0, 0);
        assert_eq!(parse_reply(&b, 7, 0), Err(ReplyError::ZeroTime));
    }

    #[test]
    fn era_1_timestamps_continue_past_2036() {
        // 2040-01-01 00:00:00 UTC = 2_208_988_800 + 2_208_988_800 + ... wraps;
        // its NTP seconds field is small, below the 1970 mark.
        let unix_2040: i64 = 2_208_988_800;
        let ntp = (unix_2040 + SECONDS_1900_TO_1970) as u64; // > 2^32
        let field = (ntp & 0xFFFF_FFFF) as u32;
        let b = reply(1, 0, MODE_SERVER, 1, field, 0);
        assert_eq!(parse_reply(&b, 1, 0), Ok(unix_2040 * 1000));
    }
}
