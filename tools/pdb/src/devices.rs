// SPDX-License-Identifier: GPL-3.0-only
//! `pdb devices` — every device on a serial port and every simulator on a
//! socket that answers a PING.

use std::io::ErrorKind;
use std::thread;
use std::time::Duration;

use serialport::SerialPortType;

use crate::protocol::{recv_response, send_frame, CMD_PING, STATUS_OK};
use crate::transport::{self, Link};

use pdb_protocol::greeting::Greeting;

// The device's descriptors are built from these same two constants, so the
// scan and the firmware cannot disagree about what a picodroid looks like.
use pdb_protocol::usb::{PID as PICODROID_PID, VID as PICODROID_VID};

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// How a simulator's row is marked, so a bench with boards attached reads
/// at a glance.
pub const SIM_TAG: &str = "[sim]";

pub fn run() {
    let devices = scan();

    if devices.is_empty() {
        println!("no picodroid devices found");
    } else {
        for (name, version) in &devices {
            println!("{name}  {version}");
        }
    }
}

/// Everything that answers a PING: devices on serial ports, then running
/// simulators.
pub fn scan() -> Vec<(String, String)> {
    let mut found = scan_serial();
    found.extend(scan_sims());
    found
}

/// Scan all serial ports and return those that respond to a picodroid PING.
/// Prefers VID/PID-based detection for USB CDC; falls back to PING probe.
fn scan_serial() -> Vec<(String, String)> {
    let ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    // Fast path: match by VID/PID first.
    let mut vid_pid_ports = Vec::new();
    let mut other_candidates = Vec::new();

    for info in ports {
        let name = &info.port_name;
        if name.contains("/tty.") || name.contains("Bluetooth") {
            continue;
        }
        if matches!(
            &info.port_type,
            SerialPortType::UsbPort(usb) if usb.vid == PICODROID_VID && usb.pid == PICODROID_PID
        ) {
            vid_pid_ports.push(info);
        } else {
            other_candidates.push(info);
        }
    }

    // Probe VID/PID matches (should respond quickly).
    let mut results: Vec<(String, String)> = vid_pid_ports
        .into_iter()
        .filter_map(|info| probe(&info.port_name).map(|ver| (info.port_name, ver)))
        .collect();

    if !results.is_empty() {
        return results;
    }

    // Fallback: probe remaining candidates in parallel.
    if other_candidates.is_empty() {
        return Vec::new();
    }

    let handles: Vec<_> = other_candidates
        .into_iter()
        .map(|info| {
            thread::spawn(move || probe(&info.port_name).map(|version| (info.port_name, version)))
        })
        .collect();

    results.extend(handles.into_iter().filter_map(|h| h.join().ok().flatten()));
    results
}

/// Every running simulator: each socket under `transport::sim_socket_dir`
/// that answers a PING, its row tagged [`SIM_TAG`]. A socket nobody listens
/// on any more — a simulator that was killed — is removed on the way past;
/// one that is busy (an install in progress) times out and is left alone.
pub fn scan_sims() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for path in transport::sim_sockets() {
        let name = path.to_string_lossy().into_owned();
        match transport::open(&name, PROBE_TIMEOUT) {
            Ok(mut link) => {
                if let Some(version) = probe_over(link.as_mut()) {
                    found.push((name, format!("{version}  {SIM_TAG}")));
                }
            }
            Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                let _ = std::fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
    found
}

/// Try a quick PING on `target`. Returns the device's [`row`] on success.
fn probe(target: &str) -> Option<String> {
    let mut link = transport::open(target, PROBE_TIMEOUT).ok()?;
    probe_over(link.as_mut())
}

fn probe_over(link: &mut dyn Link) -> Option<String> {
    send_frame(link, CMD_PING, b"").ok()?;

    let (status, payload) = recv_response(link).ok()?;
    if status != STATUS_OK {
        return None;
    }
    Greeting::parse(&payload).ok().map(|g| row(&g))
}

/// What a `devices` row says after the port: `picodroid/2.3  (pico_display2_w,
/// max PAPK: 1020 KB)`, without the board on firmware too old to send one.
/// `hil-run.sh` pins a board by the literal `max PAPK: N KB`; keep it.
fn row(g: &Greeting) -> String {
    let board = if g.board.is_empty() {
        String::new()
    } else {
        format!("{}, ", g.board)
    };
    format!("{}  ({board}max PAPK: {} KB)", g.version, g.max_papk / 1024)
}

/// Find the first serial port matching the picodroid VID/PID.
pub fn find_by_vid_pid() -> Option<String> {
    let ports = serialport::available_ports().ok()?;
    ports.into_iter().find_map(|info| {
        if matches!(
            &info.port_type,
            SerialPortType::UsbPort(usb) if usb.vid == PICODROID_VID && usb.pid == PICODROID_PID
        ) {
            Some(info.port_name)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdb_protocol::greeting::{encode, GREETING_MAX, LEGACY_GREETING_LEN, PROTOCOL_VERSION};

    #[test]
    fn a_row_names_the_board_before_the_region() {
        let mut buf = [0u8; GREETING_MAX];
        let n = encode(1020 * 1024, b"0.27.0", b"pico_display2_w", b"", &mut buf);
        let g = Greeting::parse(&buf[..n]).unwrap();
        assert_eq!(
            row(&g),
            format!("{PROTOCOL_VERSION}  (pico_display2_w, max PAPK: 1020 KB)")
        );
    }

    /// A board still on 2.2 firmware lists as it always did.
    #[test]
    fn a_row_without_a_board_reads_as_before() {
        let mut raw = vec![0u8; LEGACY_GREETING_LEN + 1];
        raw[..13].copy_from_slice(b"picodroid/2.2");
        raw[14..18].copy_from_slice(&(1020u32 * 1024).to_le_bytes());
        let g = Greeting::parse(&raw).unwrap();
        assert_eq!(row(&g), "picodroid/2.2  (max PAPK: 1020 KB)");
    }
}
