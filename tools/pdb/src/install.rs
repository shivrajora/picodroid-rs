use std::path::Path;
use std::process;
use std::time::Duration;

use crate::protocol::{
    recv_response, send_frame, send_install_data, send_install_header, status_str, CMD_PING,
    POLL_ATTEMPTS, POLL_TIMEOUT, STATUS_OK, STATUS_READY,
};

const BAUD_RATE: u32 = 115_200;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for the STATUS_READY response: device needs ~1.6 s to erase flash.
const ERASE_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for the STATUS_OK response after streaming the full PAPK.
const STREAM_TIMEOUT: Duration = Duration::from_secs(30);

/// Initial delay before polling PING after STATUS_OK.
/// Covers JVM graceful exit (~500 ms) + MCU reboot + firmware init (~1.5 s).
const REBOOT_DELAY: Duration = Duration::from_secs(3);

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

    // PING payload: "picodroid/2.0\0" (14 bytes) + max_papk_bytes u32 LE (4 bytes)
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

    // ── Phase A: send install header, wait for flash erase to finish ──────────
    port.set_timeout(ERASE_TIMEOUT).ok();

    if let Err(e) = send_install_header(port.as_mut(), papk.len() as u32) {
        eprintln!("error: INSTALL header send failed: {e}");
        process::exit(1);
    }

    println!("Erasing flash (~2 s)...");

    let (status, payload) = match recv_response(port.as_mut()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: waiting for READY: {e}");
            process::exit(1);
        }
    };

    if status != STATUS_READY {
        let msg = String::from_utf8_lossy(&payload);
        eprintln!("error: expected READY, got {} — {msg}", status_str(status));
        process::exit(1);
    }

    // ── Phase B: stream PAPK bytes + CRC32 ───────────────────────────────────
    port.set_timeout(STREAM_TIMEOUT).ok();

    println!("Streaming {} KB...", papk.len().div_ceil(1024));

    if let Err(e) = send_install_data(port.as_mut(), papk.len() as u32, &papk) {
        eprintln!("error: INSTALL data send failed: {e}");
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

    println!("PAPK written. Waiting for device to reboot...");

    // ── Poll PING until device comes back after reboot ────────────────────────
    //
    // The device reboots after the JVM exits gracefully.  pdb_task may respond
    // to a few PINGs before the reboot (while the JVM is winding down), then
    // goes silent for ~1-2 s, then comes back up.  The initial delay skips past
    // both the JVM graceful-exit phase and the reboot+reinit time.
    std::thread::sleep(REBOOT_DELAY);
    port.set_timeout(POLL_TIMEOUT).ok();

    for _ in 0..POLL_ATTEMPTS {
        std::thread::sleep(POLL_TIMEOUT);

        if send_frame(port.as_mut(), CMD_PING, b"").is_err() {
            continue;
        }

        match recv_response(port.as_mut()) {
            Ok((STATUS_OK, _)) => {
                println!("Install complete.");
                return;
            }
            _ => continue,
        }
    }

    eprintln!("warning: device did not respond to PING within 20 s after reboot.");
    eprintln!("         The install was written to flash; the app will load on next boot.");
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
