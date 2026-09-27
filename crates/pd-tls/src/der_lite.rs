// SPDX-License-Identifier: GPL-3.0-only
//! The two DER walks the trust store needs, and nothing else: where a
//! certificate's Issuer and Subject Name TLVs sit. No decoding, no OIDs —
//! `embedded-tls`'s `rustpki` does the real parsing; this only lets the
//! verifier pick a trust anchor by comparing an entry's Issuer bytes with an
//! anchor's Subject bytes, which X.509 requires to be byte-identical for a
//! well-formed chain. Shared with `build.rs` (which precomputes the anchors'
//! Subject TLVs) through `#[path]`, so it must stay dependency-free.

use core::ops::Range;

/// Byte ranges of a certificate's Issuer and Subject `Name` TLVs (tag,
/// length and value), relative to the DER the walk was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    pub issuer: Range<usize>,
    pub subject: Range<usize>,
}

/// One TLV header at `pos`: `(tag, value start, value length)`. Long-form
/// lengths up to four bytes; anything running past `buf` is `None`.
fn tlv(buf: &[u8], pos: usize) -> Option<(u8, usize, usize)> {
    let tag = *buf.get(pos)?;
    let l0 = *buf.get(pos + 1)? as usize;
    let (hdr, len) = if l0 < 0x80 {
        (2, l0)
    } else {
        let n = l0 & 0x7f;
        if n == 0 || n > 4 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | *buf.get(pos + 2 + i)? as usize;
        }
        (2 + n, len)
    };
    let start = pos + hdr;
    let end = start.checked_add(len)?;
    if end > buf.len() {
        return None;
    }
    Some((tag, start, len))
}

const SEQUENCE: u8 = 0x30;
const INTEGER: u8 = 0x02;
const VERSION_CONTEXT_0: u8 = 0xa0;

/// Locate the Issuer and Subject of a DER `Certificate`:
/// `SEQUENCE { tbsCertificate SEQUENCE { [0] version?, serialNumber,
/// signature, issuer, validity, subject, … } … }`.
pub fn cert_names(der: &[u8]) -> Option<Names> {
    let (t, cert_v, _) = tlv(der, 0)?;
    if t != SEQUENCE {
        return None;
    }
    let (t, tbs_v, tbs_len) = tlv(der, cert_v)?;
    if t != SEQUENCE {
        return None;
    }
    let tbs_end = tbs_v + tbs_len;
    let mut pos = tbs_v;

    let (t, v, l) = tlv(der, pos)?;
    if t == VERSION_CONTEXT_0 {
        pos = v + l;
    }
    let (t, v, l) = tlv(der, pos)?; // serialNumber
    if t != INTEGER {
        return None;
    }
    pos = v + l;
    let (t, v, l) = tlv(der, pos)?; // signature AlgorithmIdentifier
    if t != SEQUENCE {
        return None;
    }
    pos = v + l;
    let issuer_start = pos;
    let (t, v, l) = tlv(der, pos)?; // issuer Name
    if t != SEQUENCE {
        return None;
    }
    let issuer = issuer_start..v + l;
    pos = v + l;
    let (t, v, l) = tlv(der, pos)?; // validity
    if t != SEQUENCE {
        return None;
    }
    pos = v + l;
    let subject_start = pos;
    let (t, v, l) = tlv(der, pos)?; // subject Name
    if t != SEQUENCE {
        return None;
    }
    let subject = subject_start..v + l;
    if subject.end > tbs_end {
        return None;
    }
    Some(Names { issuer, subject })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A self-signed root: Issuer and Subject are the same bytes.
    #[test]
    fn self_signed_root_has_equal_names() {
        let der = include_bytes!("../roots/gts_root_r4.der");
        let n = cert_names(der).expect("parses");
        assert_eq!(&der[n.issuer.clone()], &der[n.subject.clone()]);
        assert_eq!(der[n.subject.start], SEQUENCE);
        assert!(n.subject.len() > 20 && n.subject.len() < 200);
    }

    #[test]
    fn every_root_parses_and_is_self_signed() {
        for der in [
            &include_bytes!("../roots/isrg_root_x1.der")[..],
            &include_bytes!("../roots/isrg_root_x2.der")[..],
            &include_bytes!("../roots/usertrust_ecc.der")[..],
            &include_bytes!("../roots/amazon_root_ca_1.der")[..],
        ] {
            let n = cert_names(der).expect("parses");
            assert_eq!(&der[n.issuer], &der[n.subject]);
        }
    }

    #[test]
    fn truncated_and_garbage_input_is_none() {
        let der = include_bytes!("../roots/gts_root_r4.der");
        assert!(cert_names(&der[..der.len() / 2]).is_none());
        assert!(cert_names(&[0x30, 0x84, 0xff, 0xff, 0xff, 0xff]).is_none());
        assert!(cert_names(&[]).is_none());
        assert!(cert_names(&[0x04, 0x01, 0x00]).is_none());
    }

    #[test]
    fn long_form_lengths_are_bounds_checked() {
        // SEQUENCE with a 2-byte length that overruns the buffer.
        assert!(tlv(&[0x30, 0x82, 0x10, 0x00, 0x00], 0).is_none());
        // Indefinite / over-long length encodings are rejected.
        assert!(tlv(&[0x30, 0x80], 0).is_none());
        assert!(tlv(&[0x30, 0x85, 0, 0, 0, 0, 1], 0).is_none());
        assert_eq!(tlv(&[0x02, 0x01, 0x07], 0), Some((0x02, 2, 1)));
    }
}
