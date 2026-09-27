// SPDX-License-Identifier: GPL-3.0-only
//! A host-side bound on the handshake's stack need: the live handshake to
//! `api.anthropic.com` on threads of shrinking stacks. Network-bound and
//! x86-64 frames are not Thumb frames, so ignored by default and read as a
//! rough figure only:
//!
//!     cargo test -p pd-tls --test stack -- --ignored --nocapture

use std::net::TcpStream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use embedded_io_adapters::std::FromStd;
use pd_tls::{TlsClock, TlsSession};

struct HostClock;
impl TlsClock for HostClock {
    fn now() -> Option<u64> {
        Some(SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs())
    }
}

fn handshake(host: &str) -> bool {
    let stream = match TcpStream::connect((host, 443)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut s = TlsSession::new(FromStd::new(stream)).expect("session alloc");
    let mut seed = [0u8; 32];
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")
            .unwrap()
            .read_exact(&mut seed)
            .unwrap();
    }
    s.open::<HostClock>(host, seed).is_ok()
}

fn on_stack(kb: usize, host: &'static str) -> bool {
    std::thread::Builder::new()
        .stack_size(kb * 1024)
        .spawn(move || handshake(host))
        .unwrap()
        .join()
        .unwrap_or(false)
}

#[test]
#[ignore]
fn handshake_stack_bound() {
    for host in ["api.anthropic.com", "api.open-meteo.com"] {
        for kb in [128usize, 64, 48, 32, 24, 16, 12] {
            let ok = on_stack(kb, host);
            println!(
                "{host}: {kb} KB stack -> {}",
                if ok { "ok" } else { "FAILED" }
            );
        }
    }
}
