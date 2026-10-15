// SPDX-License-Identifier: GPL-3.0-only
//! TLS: the bytes a server (or anyone on the path) sends during the
//! handshake, and the certificate reader the verifier uses to pick a trust
//! anchor.

use pd_tls::der_lite::cert_names;
use pd_tls::embedded_io::{self, ErrorKind, Read, Write};
use pd_tls::{NoClock, TlsSession};

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
