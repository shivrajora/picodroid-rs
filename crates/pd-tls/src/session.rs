// SPDX-License-Identifier: GPL-3.0-only
//! One TLS 1.3 client session over any blocking `embedded_io` socket.
//!
//! Everything sizeable lives on the heap and is freed with the session: the
//! two record buffers (a full 16 KB read record — public servers ignore
//! `max_fragment_length` — and a 4 KB write record, enough for any request
//! head an `HttpURLConnection` sends), the `TlsConnection` state, and, for
//! the handshake only, the crypto provider with the ~8.7 KB verifier. The
//! record buffers are leaked into `'static` slices for the connection's
//! lifetime and reclaimed in `Drop`.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::anchors;
use crate::rng::HmacDrbg;
use crate::verifier::{Failure, PdVerifier};
use embedded_io::{Read, Write};
use embedded_tls::blocking::TlsConnection;
use embedded_tls::{
    Aes128GcmSha256, CryptoProvider, CryptoRngCore, TlsClock, TlsConfig, TlsContext, TlsError,
    TlsVerifier,
};

/// A whole TLS 1.3 record (16 KB plaintext) plus the crate's overhead.
pub const READ_RECORD_BYTES: usize = 16 * 1024 + embedded_tls::TLS_RECORD_OVERHEAD;
/// Requests are small; a 4 KB record bounds each `write` call.
pub const WRITE_RECORD_BYTES: usize = 4 * 1024 + embedded_tls::TLS_RECORD_OVERHEAD;

/// The crypto provider for one handshake: the RNG and the verifier.
struct PdProvider<C: TlsClock> {
    rng: HmacDrbg,
    verifier: PdVerifier<C>,
}

/// What `TlsSession::new` and `open` put on the heap, for the alignment
/// test: the device heap aligns to 8 and nothing here may want more.
#[doc(hidden)]
pub struct Boxed {
    _conn: TlsConnection<'static, NullSocket, Aes128GcmSha256>,
    _provider: PdProvider<embedded_tls::NoClock>,
}

/// A socket that does nothing, for `Boxed`'s type only.
#[doc(hidden)]
pub struct NullSocket;
impl embedded_io::ErrorType for NullSocket {
    type Error = core::convert::Infallible;
}
impl Read for NullSocket {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        Ok(0)
    }
}
impl Write for NullSocket {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<C: TlsClock> CryptoProvider for PdProvider<C> {
    type CipherSuite = Aes128GcmSha256;
    type Signature = [u8; 0];

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.verifier)
    }
}

/// Why `open` failed: the TLS error plus, when the certificate step is what
/// failed, which check.
#[derive(Debug, Clone, Copy)]
pub struct OpenError {
    pub tls: TlsError,
    pub certificate: Failure,
}

pub struct TlsSession<S: Read + Write + 'static> {
    conn: Option<TlsConnection<'static, S, Aes128GcmSha256>>,
    read_buf: *mut [u8],
    write_buf: *mut [u8],
}

/// A dotted-quad IPv4 host, which gets no SNI and an iPAddress match.
fn is_ipv4_literal(host: &str) -> bool {
    let mut parts = 0;
    for part in host.split('.') {
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        if part.parse::<u16>().map(|v| v > 255).unwrap_or(true) {
            return false;
        }
        parts += 1;
    }
    parts == 4
}

/// A zeroed heap buffer as a `'static` slice; `OutOfMemory` instead of an
/// allocation abort when the arena cannot spare it.
fn leak_buf(len: usize) -> Result<&'static mut [u8], TlsError> {
    let mut v: Vec<u8> = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| TlsError::OutOfMemory)?;
    v.resize(len, 0);
    Ok(Box::leak(v.into_boxed_slice()))
}

impl<S: Read + Write + 'static> TlsSession<S> {
    /// Allocate the session around a connected socket. Nothing is sent
    /// until [`open`](Self::open).
    pub fn new(socket: S) -> Result<Box<Self>, TlsError> {
        let read_buf = leak_buf(READ_RECORD_BYTES)?;
        let write_buf = match leak_buf(WRITE_RECORD_BYTES) {
            Ok(b) => b,
            Err(e) => {
                // SAFETY: `read_buf` was leaked by `leak_buf` just above and
                // has no other owner.
                drop(unsafe { Box::from_raw(read_buf as *mut [u8]) });
                return Err(e);
            }
        };
        let read_ptr = read_buf as *mut [u8];
        let write_ptr = write_buf as *mut [u8];
        let conn = TlsConnection::new(socket, read_buf, write_buf);
        Ok(Box::new(Self {
            conn: Some(conn),
            read_buf: read_ptr,
            write_buf: write_ptr,
        }))
    }

    /// Run the handshake: SNI and host-name check against `host`, the
    /// chain against the compiled-in anchors, validity against `C`, key
    /// exchange randomness from `seed`.
    pub fn open<C: TlsClock>(&mut self, host: &str, seed: [u8; 32]) -> Result<(), OpenError> {
        let mut provider: Box<PdProvider<C>> = Box::new(PdProvider {
            rng: HmacDrbg::from_seed(seed),
            verifier: PdVerifier::new(anchors::ANCHORS),
        });
        // RFC 6066 forbids an IP literal in SNI; the verifier still gets it,
        // and matches it against the certificate's iPAddress entries.
        let config = if is_ipv4_literal(host) {
            provider
                .verifier
                .set_hostname_verification(host)
                .map_err(|tls| OpenError {
                    tls,
                    certificate: Failure::None,
                })?;
            TlsConfig::new()
        } else {
            TlsConfig::new().with_server_name(host)
        };
        #[cfg(feature = "rsa")]
        let config = config.enable_rsa_signatures();
        let conn = self.conn.as_mut().expect("session open after close");
        // `&mut *provider`: the context takes the provider by value, and a
        // by-value provider would put the verifier's chain copy on the
        // handshake's stack.
        conn.open(TlsContext::new(&config, &mut *provider))
            .map_err(|tls| OpenError {
                tls,
                certificate: provider.verifier.failure(),
            })
    }

    /// Application data; `Ok(0)` once the peer sent `close_notify` or the
    /// socket reached end of stream.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, TlsError> {
        self.conn
            .as_mut()
            .expect("session read after close")
            .read(buf)
    }

    /// Queue application data; bounded by the write record, so callers loop.
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, TlsError> {
        self.conn
            .as_mut()
            .expect("session write after close")
            .write(buf)
    }

    /// Encrypt and send what `write` queued.
    pub fn flush(&mut self) -> Result<(), TlsError> {
        self.conn
            .as_mut()
            .expect("session flush after close")
            .flush()
    }

    /// Send `close_notify` in place (fork patch 4: no move of the
    /// connection through the caller's stack, which is a pool worker's
    /// after an HTTP fetch) and drop the connection; the caller closes the
    /// socket it still holds. A second call is a no-op.
    pub fn close(&mut self) -> Result<(), TlsError> {
        match self.conn.as_mut() {
            Some(conn) => {
                let r = conn.close_notify();
                self.conn = None;
                r
            }
            None => Ok(()),
        }
    }
}

impl<S: Read + Write + 'static> Drop for TlsSession<S> {
    fn drop(&mut self) {
        // The connection borrows the buffers: drop it first.
        drop(self.conn.take());
        // SAFETY: both pointers came from `leak_buf` in `new`, are freed
        // exactly here, and nothing borrows them once `conn` is gone.
        unsafe {
            drop(Box::from_raw(self.read_buf));
            drop(Box::from_raw(self.write_buf));
        }
    }
}
