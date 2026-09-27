// SPDX-License-Identifier: GPL-3.0-only
//! Certificate verification against the trust store.
//!
//! `embedded-tls`'s `pki::CertVerifier` takes one CA and verifies a server
//! chain from its *last* entry towards the leaf, so a chain that ends in a
//! cross-signed copy of the root (every public chain probed on 2026-09-27)
//! fails against the real root. `PdVerifier` wraps one heap-resident
//! `CertVerifier`: it finds the first entry whose Issuer is an anchor's
//! Subject, cuts the chain there, swaps that anchor in (`set_ca`, a local
//! patch) and lets the inner verifier do the signatures, the validity dates
//! and the host-name match.
//!
//! Policy: no wall clock, no handshake. `C::now()` returning `None` fails
//! the certificate step instead of skipping the validity check, which is
//! what the inner verifier would otherwise do.

use embedded_tls::pki::CertVerifier;
use embedded_tls::{
    Aes128GcmSha256, Certificate, CertificateEntryRef, CertificateRef, CertificateVerifyRef,
    TlsCipherSuite, TlsClock, TlsError, TlsVerifier,
};

use crate::anchors::{self, Anchor};

/// Bytes the inner verifier keeps of the received chain: the whole raw
/// `Certificate` message, which it re-parses for `CertificateVerify`. Public
/// chains run 2.5–4.1 KB; 8 KB leaves room for a fourth entry.
pub const CERT_SIZE: usize = 8192;

/// Why the last handshake's certificate step failed — `TlsError` alone
/// says `InvalidCertificate` for all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The verifier was never asked (the failure lies elsewhere).
    None,
    /// The wall clock has not been set: validity cannot be checked.
    NoClock,
    /// No entry of the chain is issued by any compiled-in anchor.
    NoAnchor,
    /// A chain entry could not be walked as an X.509 certificate.
    Malformed,
    /// Signature, validity or host-name check failed in the inner verifier.
    Rejected,
}

/// Multi-anchor verifier over the compiled-in trust store. About 8.7 KB
/// (the inner verifier's chain copy) — construct it on the heap.
pub struct PdVerifier<C: TlsClock> {
    anchors: &'static [Anchor],
    inner: CertVerifier<'static, Aes128GcmSha256, C, CERT_SIZE>,
    failure: Failure,
}

impl<C: TlsClock> PdVerifier<C> {
    pub fn new(anchors: &'static [Anchor]) -> Self {
        // Placeholder anchor; `verify_certificate` swaps the real one in.
        let first: &'static [u8] = anchors.first().map(|a| a.der).unwrap_or(&[]);
        Self {
            anchors,
            inner: CertVerifier::new(Certificate::X509(first)),
            failure: Failure::None,
        }
    }

    pub fn failure(&self) -> Failure {
        self.failure
    }
}

impl<C: TlsClock> TlsVerifier<Aes128GcmSha256> for PdVerifier<C> {
    fn set_hostname_verification(&mut self, hostname: &str) -> Result<(), TlsError> {
        self.inner.set_hostname_verification(hostname)
    }

    fn verify_certificate(
        &mut self,
        transcript: &<Aes128GcmSha256 as TlsCipherSuite>::Hash,
        mut cert: CertificateRef,
    ) -> Result<(), TlsError> {
        if C::now().is_none() {
            self.failure = Failure::NoClock;
            return Err(TlsError::InvalidCertificate);
        }
        let mut chosen: Option<(&'static Anchor, usize)> = None;
        for (i, entry) in cert.entries.iter().enumerate() {
            let CertificateEntryRef::X509(der) = entry else {
                self.failure = Failure::Malformed;
                return Err(TlsError::InvalidCertificateEntry);
            };
            match anchors::select(self.anchors, core::iter::once(*der)) {
                Some((a, _)) => {
                    chosen = Some((a, i + 1));
                    break;
                }
                None if crate::der_lite::cert_names(der).is_none() => {
                    self.failure = Failure::Malformed;
                    return Err(TlsError::DecodeError);
                }
                None => {}
            }
        }
        let Some((anchor, keep)) = chosen else {
            self.failure = Failure::NoAnchor;
            return Err(TlsError::InvalidCertificate);
        };
        cert.entries.truncate(keep);
        self.inner.set_ca(Certificate::X509(anchor.der));
        self.inner
            .verify_certificate(transcript, cert)
            .inspect_err(|_| self.failure = Failure::Rejected)
    }

    fn verify_signature(&mut self, verify: CertificateVerifyRef) -> Result<(), TlsError> {
        self.inner
            .verify_signature(verify)
            .inspect_err(|_| self.failure = Failure::Rejected)
    }
}
