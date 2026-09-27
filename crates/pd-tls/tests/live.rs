// SPDX-License-Identifier: GPL-3.0-only
//! Handshakes against public hosts — the trust store and the multi-anchor
//! verifier against real chains (ECDSA under GTS Root R4 / USERTrust ECC,
//! RSA under ISRG Root X1). Network-bound, so ignored by default:
//!
//!     cargo test -p pd-tls --test live -- --ignored --nocapture

use std::net::TcpStream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use embedded_io_adapters::std::FromStd;
use pd_tls::{Failure, TlsClock, TlsSession};

struct HostClock;
impl TlsClock for HostClock {
    fn now() -> Option<u64> {
        Some(SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs())
    }
}

/// A clock that has never been set: the verifier must refuse to proceed.
struct NoWallClock;
impl TlsClock for NoWallClock {
    fn now() -> Option<u64> {
        None
    }
}

fn seed() -> [u8; 32] {
    use std::io::Read;
    let mut s = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut s)
        .unwrap();
    s
}

fn connect(host: &str) -> FromStd<TcpStream> {
    let stream = TcpStream::connect((host, 443)).expect("tcp connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    FromStd::new(stream)
}

/// GET / and return the status line.
fn get_status<C: TlsClock>(host: &str, sni: &str) -> Result<String, pd_tls::OpenError> {
    let mut s = TlsSession::new(connect(host)).expect("session alloc");
    let t0 = std::time::Instant::now();
    s.open::<C>(sni, seed())?;
    let handshake = t0.elapsed();
    let req = format!("GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    let mut off = 0;
    while off < req.len() {
        off += s.write(&req.as_bytes()[off..]).expect("write");
    }
    s.flush().expect("flush");
    let mut buf = [0u8; 2048];
    let n = s.read(&mut buf).expect("read");
    let head = String::from_utf8_lossy(&buf[..n]);
    let status = head.lines().next().unwrap_or("").to_string();
    println!("{host}: handshake {handshake:?}, {status}");
    let _ = s.close();
    Ok(status)
}

#[test]
#[ignore]
fn anthropic_ecdsa_chain_under_gts_root_r4() {
    let status = get_status::<HostClock>("api.anthropic.com", "api.anthropic.com").unwrap();
    assert!(status.starts_with("HTTP/1.1 "), "{status}");
}

#[test]
#[ignore]
fn github_ecdsa_chain_under_usertrust_ecc() {
    let status = get_status::<HostClock>("api.github.com", "api.github.com").unwrap();
    assert!(status.starts_with("HTTP/1.1 "), "{status}");
}

#[test]
#[ignore]
fn open_meteo_rsa_chain_under_isrg_root_x1() {
    let status = get_status::<HostClock>("api.open-meteo.com", "api.open-meteo.com").unwrap();
    assert!(status.starts_with("HTTP/1.1 "), "{status}");
}

#[test]
#[ignore]
fn wrong_host_name_is_rejected() {
    // A front end serves a different certificate for an unknown SNI, so the
    // rejection is either the host-name mismatch or a chain no anchor issued;
    // both are the certificate step refusing.
    let err = get_status::<HostClock>("api.anthropic.com", "example.com").unwrap_err();
    assert!(
        matches!(err.certificate, Failure::Rejected | Failure::NoAnchor),
        "{err:?}"
    );
}

#[test]
#[ignore]
fn unset_wall_clock_fails_closed() {
    let err = get_status::<NoWallClock>("api.anthropic.com", "api.anthropic.com").unwrap_err();
    assert_eq!(err.certificate, Failure::NoClock, "{err:?}");
}
