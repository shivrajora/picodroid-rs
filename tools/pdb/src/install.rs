use std::path::Path;
use std::process;
use std::time::Duration;

use crate::protocol::{
    recv_response, send_frame, status_str, CMD_INSTALL, CMD_PING, POLL_ATTEMPTS, POLL_TIMEOUT,
    STATUS_OK,
};

const BAUD_RATE: u32 = 115_200;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run(port_name: &str, papk_path: &Path) {
    // ── Read PAPK file ────────────────────────────────────────────────────────
    let papk = match std::fs::read(papk_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", papk_path.display());
            process::exit(1);
        }
    };

    // ── Open serial port ──────────────────────────────────────────────────────
    let mut port = match serialport::new(port_name, BAUD_RATE)
        .timeout(CONNECT_TIMEOUT)
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot open {port_name}: {e}");
            process::exit(1);
        }
    };

    // ── PING — identify device and get max PAPK size ──────────────────────────
    if let Err(e) = send_frame(port.as_mut(), CMD_PING, b"") {
        eprintln!("error: PING send failed: {e}");
        process::exit(1);
    }

    let (status, ping_payload) = match recv_response(port.as_mut()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: PING response failed: {e}");
            eprintln!("       Is the device connected and running picodroid firmware?");
            process::exit(1);
        }
    };

    if status != STATUS_OK {
        eprintln!("error: PING returned {}", status_str(status));
        process::exit(1);
    }

    // PING payload: "picodroid/1.0\0" (14 bytes) + max_papk_bytes u32 LE (4 bytes)
    if ping_payload.len() < 18 {
        eprintln!(
            "error: PING payload too short ({} bytes)",
            ping_payload.len()
        );
        process::exit(1);
    }

    let version = std::str::from_utf8(&ping_payload[..14])
        .unwrap_or("?")
        .trim_end_matches('\0');
    let max_papk = u32::from_le_bytes([
        ping_payload[14],
        ping_payload[15],
        ping_payload[16],
        ping_payload[17],
    ]) as usize;

    println!("Connected: {version}  (max PAPK: {} KB)", max_papk / 1024);

    // ── Validate size ─────────────────────────────────────────────────────────
    if papk.len() > max_papk {
        eprintln!(
            "error: PAPK is {} KB but device supports max {} KB",
            papk.len().div_ceil(1024),
            max_papk / 1024,
        );
        process::exit(1);
    }

    println!(
        "Installing {} ({} KB)...",
        papk_path.display(),
        papk.len().div_ceil(1024)
    );

    // ── INSTALL ───────────────────────────────────────────────────────────────
    if let Err(e) = send_frame(port.as_mut(), CMD_INSTALL, &papk) {
        eprintln!("error: INSTALL send failed: {e}");
        process::exit(1);
    }

    let (status, payload) = match recv_response(port.as_mut()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: INSTALL response failed: {e}");
            process::exit(1);
        }
    };

    if status != STATUS_OK {
        let msg = String::from_utf8_lossy(&payload);
        eprintln!("error: INSTALL returned {} — {msg}", status_str(status));
        process::exit(1);
    }

    println!("PAPK received. Waiting for app to start...");

    // ── Poll PING until the new app responds ──────────────────────────────────
    port.set_timeout(POLL_TIMEOUT).ok();

    for _ in 0..POLL_ATTEMPTS {
        std::thread::sleep(POLL_TIMEOUT);

        if send_frame(port.as_mut(), CMD_PING, b"").is_err() {
            continue;
        }

        // Drain any stale bytes by reading with a short buffer until we find the magic.
        // recv_response will return an error if magic is wrong — just retry.
        match recv_response(port.as_mut()) {
            Ok((STATUS_OK, _)) => {
                println!("Install complete.");
                return;
            }
            _ => continue,
        }
    }

    eprintln!("warning: app did not respond to PING within 5 s.");
    eprintln!("         The install was sent; the app may still be starting.");
}

pub fn ping(port_name: &str) {
    let mut port = match serialport::new(port_name, BAUD_RATE)
        .timeout(CONNECT_TIMEOUT)
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot open {port_name}: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = send_frame(port.as_mut(), CMD_PING, b"") {
        eprintln!("error: PING send failed: {e}");
        process::exit(1);
    }

    match recv_response(port.as_mut()) {
        Ok((STATUS_OK, payload)) if payload.len() >= 18 => {
            let version = std::str::from_utf8(&payload[..14])
                .unwrap_or("?")
                .trim_end_matches('\0');
            let max_papk =
                u32::from_le_bytes([payload[14], payload[15], payload[16], payload[17]]) as usize;
            println!("{version}  (max PAPK: {} KB)", max_papk / 1024);
        }
        Ok((status, _)) => {
            eprintln!("error: PING returned {}", status_str(status));
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    }
}
