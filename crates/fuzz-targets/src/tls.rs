// SPDX-License-Identifier: GPL-3.0-only
//! TLS: the bytes a server (or anyone on the path) sends during the
//! handshake, the certificate reader the verifier uses to pick a trust
//! anchor, and the verifier itself over a decrypted Certificate message.

use pd_tls::anchors::{select, ANCHORS};
use pd_tls::der_lite::cert_names;
use pd_tls::embedded_io::{self, ErrorKind, Read, Write};
use pd_tls::{
    Aes128GcmSha256, CertificateEntryRef, CertificateRef, Failure, NoClock, PdVerifier,
    TlsCipherSuite, TlsClock, TlsSession, TlsVerifier,
};

use crate::Bytes;

/// Any bytes as a DER certificate. The names it finds lie inside the
/// certificate and are DER `SEQUENCE`s.
pub fn der(data: &[u8]) {
    if let Some(n) = cert_names(data) {
        for r in [&n.issuer, &n.subject] {
            assert!(
                r.start < r.end && r.end <= data.len(),
                "{r:?} of {}",
                data.len()
            );
            assert_eq!(data[r.start], 0x30, "a name that is not a SEQUENCE");
        }
    }
}

/// A socket whose server half is the fuzz input, handed out in reads of
/// input-chosen sizes, and whose end is an error: embedded-tls treats a
/// zero-length read as "try again", and a test socket that never closes
/// would spin.
struct Scripted {
    rx: Vec<u8>,
    pos: usize,
    sizes: Vec<u8>,
    reads: usize,
}

impl embedded_io::ErrorType for Scripted {
    type Error = ErrorKind;
}

impl Read for Scripted {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ErrorKind> {
        if self.pos >= self.rx.len() || buf.is_empty() {
            return Err(ErrorKind::ConnectionReset);
        }
        let size = self
            .sizes
            .get(self.reads % self.sizes.len().max(1))
            .copied();
        self.reads += 1;
        let want = size.map_or(buf.len(), |s| (s as usize).max(1));
        let n = want.min(buf.len()).min(self.rx.len() - self.pos);
        buf[..n].copy_from_slice(&self.rx[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

impl Write for Scripted {
    fn write(&mut self, buf: &[u8]) -> Result<usize, ErrorKind> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), ErrorKind> {
        Ok(())
    }
}

/// Input: a byte giving how many read sizes follow, the sizes, then the
/// server's half of a handshake. The client sends its ClientHello into the
/// void and parses whatever comes back: ServerHello, HelloRetryRequest,
/// alerts, record framing. Without the server's keys the fuzzer cannot
/// forge the encrypted half of the handshake, so this reaches the
/// plaintext record layer and the decrypt-failure path — the part a
/// network attacker reaches too. It must fail cleanly, never panic.
pub fn tls_server(data: &[u8]) {
    let (head, rest) = data.split_first().unwrap_or((&0, &[]));
    let n = (*head as usize % 8).min(rest.len());
    let (sizes, rx) = rest.split_at(n);
    let socket = Scripted {
        rx: rx.to_vec(),
        pos: 0,
        sizes: sizes.to_vec(),
        reads: 0,
    };
    let mut session = TlsSession::new(socket).expect("session buffers");
    let opened = session.open::<NoClock>("example.com", [7; 32]);
    // No clock, so no certificate can verify: the handshake cannot finish.
    assert!(
        opened.is_err(),
        "a handshake finished with no clock to check validity"
    );
}

thread_local! {
    /// The wall clock `FixedClock` reports: `0` means "never set".
    static CLOCK: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

/// A clock the input sets: `TlsClock::now` has no receiver, so the value
/// rides in a thread-local.
struct FixedClock;

impl TlsClock for FixedClock {
    fn now() -> Option<u64> {
        let v = CLOCK.with(|c| c.get());
        (v != 0).then_some(v)
    }
}

/// Longest DER entry the input can spell before the last one, which takes
/// the rest. Public chain entries run under 2 KB.
pub const ENTRY_MAX: usize = 8192;

/// Input: a `u64` wall clock (0 = unset), a host name, an entry count, then
/// that many DER blobs — the server's Certificate message, as `PdVerifier`
/// sees it once the record layer has decrypted it. This is the first code
/// that reads a hostile server's certificate bytes: DER walking, CN/SAN
/// extraction and the validity dates all run before any signature check.
///
/// The oracle is the cheaper reader the verifier itself uses to choose an
/// anchor: `anchors::select` over `der_lite::cert_names`. The verifier's
/// `failure()` must agree with it — `NoClock` with no clock, `Malformed`
/// when an entry before the first anchor-issued one is not a certificate
/// our reader can walk, `NoAnchor` when none is, and `Rejected` or `Ok`
/// only once an anchor was chosen. `Ok` leaves `failure()` at `None`.
pub fn tls_verify(data: &[u8]) {
    let mut b = Bytes::new(data);
    let clock = b.u64();
    let host = String::from_utf8_lossy(b.take(64)).into_owned();
    let n = b.u8() as usize % 5;
    let mut ders: Vec<&[u8]> = Vec::with_capacity(n);
    for i in 0..n {
        ders.push(if i + 1 == n {
            b.rest()
        } else {
            b.take(ENTRY_MAX)
        });
    }
    CLOCK.with(|c| c.set(clock));

    let mut cert = CertificateRef::with_context(&[]);
    for der in &ders {
        cert.add(CertificateEntryRef::X509(der))
            .expect("under 16 entries");
    }
    // ~8.7 KB; the session boxes it too.
    let mut v = Box::new(PdVerifier::<FixedClock>::new(ANCHORS));
    let host_set = v.set_hostname_verification(&host).is_ok();
    let transcript = <Aes128GcmSha256 as TlsCipherSuite>::Hash::default();
    let result = v.verify_certificate(&transcript, cert);

    let chosen = select(ANCHORS, ders.iter().copied());
    let expected = if clock == 0 {
        Some(Failure::NoClock)
    } else {
        match chosen {
            Some(_) => None,
            None if ders.iter().any(|d| cert_names(d).is_none()) => Some(Failure::Malformed),
            None => Some(Failure::NoAnchor),
        }
    };
    match (result, v.failure()) {
        (Ok(()), f) => {
            assert_eq!(f, Failure::None, "accepted a chain but recorded {f:?}");
            assert_eq!(expected, None, "accepted a chain the anchor reader refuses");
            assert!(host_set, "accepted a chain with no host name to match");
            let (_, keep) = chosen.unwrap();
            for der in &ders[..keep] {
                assert!(
                    cert_names(der).is_some(),
                    "accepted an entry our reader cannot walk"
                );
            }
        }
        (Err(_), f) => {
            assert_ne!(f, Failure::None, "failed without saying why");
            match expected {
                Some(e) => assert_eq!(f, e, "anchor reader and verifier disagree"),
                None => assert_eq!(f, Failure::Rejected, "an anchored chain failed early"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: [&[u8]; 3] = [
        include_bytes!("../tests/fixtures/chain-api.anthropic.com/0.der"),
        include_bytes!("../tests/fixtures/chain-api.anthropic.com/1.der"),
        include_bytes!("../tests/fixtures/chain-api.anthropic.com/2.der"),
    ];

    fn verify(clock: u64, host: &str, ders: &[&[u8]]) -> (bool, Failure) {
        CLOCK.with(|c| c.set(clock));
        let mut cert = CertificateRef::with_context(&[]);
        for der in ders {
            cert.add(CertificateEntryRef::X509(der)).unwrap();
        }
        let mut v = Box::new(PdVerifier::<FixedClock>::new(ANCHORS));
        v.set_hostname_verification(host).unwrap();
        let transcript = <Aes128GcmSha256 as TlsCipherSuite>::Hash::default();
        let ok = v.verify_certificate(&transcript, cert).is_ok();
        (ok, v.failure())
    }

    /// The seed chain is the corpus's one accepted input: the target must
    /// start from a verified chain, not rediscover one.
    #[test]
    fn the_captured_chain_verifies_and_each_seed_fails_as_named() {
        assert_eq!(
            verify(1_792_473_544, "api.anthropic.com", &CHAIN),
            (true, Failure::None)
        );
        assert_eq!(
            verify(1_792_473_544, "example.com", &CHAIN),
            (false, Failure::Rejected)
        );
        assert_eq!(
            verify(1_700_000_000, "api.anthropic.com", &CHAIN),
            (false, Failure::Rejected)
        );
        assert_eq!(
            verify(0, "api.anthropic.com", &CHAIN),
            (false, Failure::NoClock)
        );
        assert_eq!(
            verify(1_792_473_544, "api.anthropic.com", &CHAIN[..1]),
            (false, Failure::NoAnchor)
        );
        assert_eq!(
            verify(
                1_792_473_544,
                "api.anthropic.com",
                &[&[0x30, 0x00], CHAIN[1]]
            ),
            (false, Failure::Malformed)
        );
    }
}
