// SPDX-License-Identifier: GPL-3.0-only
//! The small decoders of `pd-codec`: a network packet, the `/system/*`
//! store records, an app's path, a compiled-layout node. Each is a few
//! dozen lines that core trusts without a second check, so each target
//! re-derives what the decoder should say and compares.

use pd_codec::inflate_scan::{self, MAX_DEFERRED};
use pd_codec::sandbox::{self, Rejected, APP_PATH_MAX, BUF, DATA_ROOT};
use pd_codec::serial::{Serial, ID_BYTES_MAX, SERIAL_MAX};
use pd_codec::sntp::{self, ReplyError, PACKET_LEN};
use pd_codec::time::{self, valid_offset, Record};
use pd_codec::wifi::{self, Credentials, Security, ENCODED_MAX, PASS_MAX, SSID_MAX};

use crate::Bytes;

/// Input: the nonce the request carried, a round-trip time, then the
/// datagram that came back.
///
/// `parse_reply` says `Ok` only for a 48-byte server-mode packet from a
/// synchronised server that echoes the nonce and has a transmit time; the
/// harness reads those fields itself and expects the same verdict, in the
/// decoder's order of checks. A reply forged from the input by fixing just
/// those fields must parse.
pub fn sntp(data: &[u8]) {
    let mut b = Bytes::new(data);
    let nonce = b.u64();
    let rtt_ms = i64::from(b.u16());
    let reply = b.rest();

    let req = sntp::request(nonce);
    assert_eq!(req.len(), PACKET_LEN);
    assert_eq!(req[0], 0x23, "version 4, client mode");
    assert_eq!(&req[40..48], &nonce.to_be_bytes());
    assert!(req[1..40].iter().all(|&x| x == 0));

    let expected = expected_reply(reply, nonce);
    let got = sntp::parse_reply(reply, nonce, rtt_ms);
    match (expected, got) {
        (Err(e), Err(g)) => assert_eq!(e, g, "refusal named differently"),
        (Ok(ms), Ok(g)) => assert_eq!(ms + rtt_ms / 2, g),
        (e, g) => panic!("harness says {e:?}, decoder says {g:?}"),
    }

    // The same bytes with the fields a good server fills: always a reply.
    if reply.len() >= PACKET_LEN {
        let mut good = reply[..PACKET_LEN].to_vec();
        good[0] = (good[0] & 0b0011_1000) | 4; // LI 0, mode 4
        good[1] = good[1].clamp(1, 15);
        good[24..32].copy_from_slice(&nonce.to_be_bytes());
        // A transmit time of exactly 1970-01-01T00:00:00.000 reads as the
        // zero timestamp too (the decoder answers in Unix milliseconds, and
        // 0 is its "unset"); give such a reply a time the decoder keeps.
        if expected_reply(&good, nonce) == Err(ReplyError::ZeroTime) {
            good[40..44].copy_from_slice(&0xE000_0000u32.to_be_bytes());
        }
        assert!(
            sntp::parse_reply(&good, nonce, rtt_ms).is_ok(),
            "a well-formed reply was refused: {good:?}"
        );
    }
}

/// RFC 4330 by hand: what `parse_reply` must say, and the transmit time in
/// Unix milliseconds when it says yes.
fn expected_reply(buf: &[u8], nonce: u64) -> Result<i64, ReplyError> {
    if buf.len() < PACKET_LEN {
        return Err(ReplyError::Short);
    }
    if buf[0] & 7 != 4 {
        return Err(ReplyError::NotServer);
    }
    if buf[0] >> 6 == 3 || buf[1] == 0 || buf[1] > 15 {
        return Err(ReplyError::Unsynchronised);
    }
    if u64::from_be_bytes(buf[24..32].try_into().unwrap()) != nonce {
        return Err(ReplyError::Nonce);
    }
    let secs = u32::from_be_bytes(buf[40..44].try_into().unwrap());
    let frac = u32::from_be_bytes(buf[44..48].try_into().unwrap());
    if secs == 0 {
        return Err(ReplyError::ZeroTime);
    }
    let mut secs = i64::from(secs);
    if secs < 2_208_988_800 {
        secs += 1 << 32;
    }
    let ms = (secs - 2_208_988_800) * 1000 + ((i64::from(frac) * 1000) >> 32);
    // The Unix epoch itself is indistinguishable from "unset" in the
    // decoder's millisecond answer, so it is refused the same way.
    if ms == 0 {
        return Err(ReplyError::ZeroTime);
    }
    Ok(ms)
}

/// Input: a selector, then one of the `/system/*` records or an id.
///
/// Whatever decodes re-encodes to the bytes it came from, and whatever the
/// harness builds within the caps decodes back to itself: the store never
/// accepts a record it could not have written, and never refuses one it did.
pub fn sysfmt(data: &[u8]) {
    let mut b = Bytes::new(data);
    match b.u8() % 5 {
        0 => wifi_record(b.rest()),
        1 => wifi_build(&mut b),
        2 => time_record(b.rest()),
        3 => zone_id(b.rest()),
        _ => serial(&mut b),
    }
}

fn wifi_record(bytes: &[u8]) {
    let Some(c) = wifi::decode(bytes) else {
        return;
    };
    assert!(!c.ssid().is_empty() && c.ssid().len() <= SSID_MAX);
    assert!(c.pass().len() <= PASS_MAX);
    let mut out = [0u8; ENCODED_MAX];
    let n = wifi::encode(&c, &mut out);
    assert!(n <= ENCODED_MAX);
    assert!(n <= bytes.len(), "decoded more than was there");
    assert_eq!(&out[..n], &bytes[..n], "decode -> encode is not identity");
    assert_eq!(wifi::decode(&out[..n]), Some(c));
}

fn wifi_build(b: &mut Bytes<'_>) {
    let ssid = b.take(SSID_MAX + 2);
    let pass = b.take(PASS_MAX + 2);
    let sec = Security::from_u8(b.u8() % 6).unwrap();
    let c = Credentials::new(ssid, pass, sec);
    assert_eq!(
        c.is_some(),
        !ssid.is_empty() && ssid.len() <= SSID_MAX && pass.len() <= PASS_MAX
    );
    let Some(c) = c else {
        return;
    };
    assert_eq!((c.ssid(), c.pass(), c.security), (ssid, pass, sec));
    let mut out = [0u8; ENCODED_MAX];
    let n = wifi::encode(&c, &mut out);
    assert_eq!(n, 8 + ssid.len() + pass.len() + 4);
    assert_eq!(wifi::decode(&out[..n]), Some(c));
    // One flipped bit anywhere, and the record is refused.
    if n > 0 {
        let at = b.below(n);
        let mut bad = out;
        bad[at] ^= 1 << (b.u8() % 8);
        assert_eq!(wifi::decode(&bad[..n]), None, "corruption at {at} passed");
    }
}

fn time_record(bytes: &[u8]) {
    let Some(r) = Record::decode(bytes) else {
        assert!(
            bytes.len() < time::RECORD_LEN
                || !valid_offset(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
            "a well-formed record was refused"
        );
        return;
    };
    assert!(valid_offset(r.offset_minutes));
    assert_eq!(Record::decode(&r.encode()), Some(r));
    assert_eq!(&r.encode()[..4], &bytes[..4]);
    assert_eq!(r.auto_time, bytes[4] != 0);
}

fn zone_id(bytes: &[u8]) {
    let Ok(id) = core::str::from_utf8(bytes) else {
        return;
    };
    let parsed = time::parse_offset_minutes(id);
    if let Some(m) = parsed {
        assert!(valid_offset(m), "offset {m} out of range");
        // The three UTC spellings and the bare offset agree.
        let t = id.trim();
        let rest = ["UTC", "GMT", "UT"]
            .iter()
            .find_map(|p| t.strip_prefix(p))
            .unwrap_or(t);
        if t != "Z" {
            // A bare offset needs a sign; the empty id is not one.
            let bare: &[&str] = if rest.is_empty() { &[] } else { &[""] };
            for p in ["UTC", "GMT", "UT"].iter().chain(bare) {
                assert_eq!(
                    time::parse_offset_minutes(&format!("{p}{rest}")),
                    Some(m),
                    "{p}{rest}"
                );
            }
        }
    }
    // A region id never parses; neither does anything with a letter after
    // the prefix.
    if id.contains('/') {
        assert_eq!(parsed, None, "a region id parsed");
    }
}

fn serial(b: &mut Bytes<'_>) {
    if b.u8() & 1 == 0 {
        let id = b.take(ID_BYTES_MAX + 2);
        let s = Serial::from_id_bytes(id);
        let blank = id.iter().all(|&x| x == 0) || id.iter().all(|&x| x == 0xFF);
        assert_eq!(
            s.is_some(),
            !id.is_empty() && id.len() <= ID_BYTES_MAX && !blank
        );
        if let Some(s) = s {
            let hex = s.as_str();
            assert_eq!(hex.len(), 2 * id.len());
            assert!(hex
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c)));
            assert_eq!(Serial::from_ascii(hex), Some(s));
            assert!(s.is_known());
        }
    } else {
        // `/system/serial` read back: text, or it is not a serial.
        let raw = b.rest();
        let Ok(text) = core::str::from_utf8(raw) else {
            return;
        };
        let s = Serial::from_ascii(text);
        let ok = !raw.is_empty()
            && raw.len() <= SERIAL_MAX
            && raw
                .iter()
                .all(|&c| c.is_ascii_alphanumeric() || b"._:-".contains(&c));
        assert_eq!(s.is_some(), ok, "{raw:?}");
        if let Some(s) = s {
            assert_eq!(s.as_bytes(), raw);
            assert_eq!(s.as_str(), text);
            // The published copy the firmware keeps reads back as itself.
            let mut buf = [0u8; SERIAL_MAX];
            buf[..raw.len()].copy_from_slice(raw);
            assert_eq!(Serial::from_published(buf, raw.len() as u8), s);
        }
    }
}

/// Input: a package name length and the name, then the app's path.
///
/// `resolve` is the one seam between an app and the volume. An accepted
/// path is exactly `/data/<package>` plus the path's real segments, never
/// climbs, and is the UTF-8 the `from_utf8_unchecked` inside claims it is;
/// a refusal names the right reason.
pub fn sandbox(data: &[u8]) {
    let mut b = Bytes::new(data);
    let with_package = b.u8() & 1 == 1;
    let package = b.take(sandbox::PACKAGE_MAX + 8);
    let path = b.rest();
    let (Ok(package), Ok(path)) = (core::str::from_utf8(package), core::str::from_utf8(path))
    else {
        return;
    };
    let mut buf = [0u8; BUF];
    let got = sandbox::resolve(with_package.then_some(package), path, &mut buf);

    if !with_package {
        match got {
            Ok(p) => assert_eq!(p, path),
            Err(e) => assert!(path.len() > BUF && e == Rejected::TooLong, "{e:?}"),
        }
        return;
    }

    let climbs = path.split('/').any(|s| s == "..");
    let mut expected = String::from(DATA_ROOT);
    expected.push('/');
    expected.push_str(package);
    for segment in path.split('/') {
        if !segment.is_empty() && segment != "." && segment != ".." {
            expected.push('/');
            expected.push_str(segment);
        }
    }
    match got {
        Ok(p) => {
            assert!(
                core::str::from_utf8(p.as_bytes()).is_ok(),
                "the SAFETY claim"
            );
            assert!(p.len() <= BUF);
            assert!(!climbs, "a climbing path was accepted: {path:?}");
            assert!(path.len() <= APP_PATH_MAX);
            assert_eq!(p, expected);
            assert!(p.starts_with(&format!("{DATA_ROOT}/{package}")));
        }
        Err(Rejected::Climb) => assert!(climbs, "Climb without a `..`: {path:?}"),
        Err(Rejected::TooLong) => assert!(
            path.len() > APP_PATH_MAX || expected.len() > BUF,
            "TooLong for {} bytes of path and {} of result",
            path.len(),
            expected.len()
        ),
    }
}

/// Input: little-endian words of a compiled layout, walked from word 0 as
/// the inflater walks children and siblings.
///
/// A node's `next_pos` is its header plus its attribute pairs, nothing
/// else; the deferred list never overflows; and the `LayoutParams`
/// arithmetic the inflater runs on every node (`frame_offset`,
/// `milli_weight`, `pack_line_mode`, `text_gravity`) takes any words a
/// PAPK can hold. The last one is what found `frame_offset`'s overflow.
pub fn layout_node(data: &[u8]) {
    let words: Vec<u32> = data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let word = |i: usize| words.get(i).copied();
    let mut pos = 0usize;
    let mut nodes = 0usize;
    while let Ok(n) = inflate_scan::scan(word, pos) {
        nodes += 1;
        let header = words[pos];
        let attrs = ((header >> 8) & 0xFF) as usize;
        assert_eq!(n.cls, (header & 0xFF) as u8);
        assert_eq!(n.child_count, header >> 16);
        assert_eq!(n.next_pos, pos + 1 + 2 * attrs, "next_pos");
        assert!(n.next_pos <= words.len());
        assert!(n.deferred_count <= MAX_DEFERRED);
        if n.too_many_deferred {
            assert_eq!(n.deferred_count, MAX_DEFERRED);
        }
        assert!(
            n.deferred_count <= attrs + 4,
            "deferred more than was there"
        );
        let _ = inflate_scan::frame_offset(n.layout_gravity, n.margin);
        let _ = inflate_scan::milli_weight(n.weight_bits);
        let _ = inflate_scan::text_gravity(n.gravity.unwrap_or(0));
        let _ = inflate_scan::pack_line_mode(
            n.ellipsize.unwrap_or(0),
            n.single_line.unwrap_or(false),
            n.max_lines.unwrap_or(0),
            n.include_font_padding == Some(false),
            n.text_color.is_some(),
        );
        let _ = (
            inflate_scan::is_fast_class(n.cls),
            inflate_scan::is_text_class(n.cls),
            inflate_scan::is_linear_class(n.cls),
            inflate_scan::is_drawing_class(n.cls),
        );
        pos = n.next_pos;
        if nodes > words.len() {
            panic!("the walk did not advance");
        }
    }
    // Truncated exactly where the words run out, not before.
    if pos < words.len() {
        let attrs = ((words[pos] >> 8) & 0xFF) as usize;
        assert!(pos + 1 + 2 * attrs > words.len(), "refused a complete node");
    }
}
