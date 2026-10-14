// SPDX-License-Identifier: GPL-3.0-only
//! `pdb devices [-l]` — every picodroid on a USB port and every simulator on
//! a socket, one row each, under adb's `List of devices attached` line and a
//! row naming the columns:
//!
//! ```text
//! SERIAL            STATE   BOARD             SPACE AVAILABLE
//! E66258881755602D  device  testbench_rp2040           764 KB
//! 1C05163AC1ACB0AA  device  pico_touch_kit            9720 KB
//! ```
//!
//! adb stops at `<serial>\t<state>`; the board and the space are
//! ours, kept because they are what a bench of several boards is told apart
//! by at a glance. The serial and the state are still the first two fields,
//! so a script's `$1`/`$2` reads the same as for adb (and `$2 == "device"`
//! skips the column row).
//!
//! - STATE: `device` when it answers a PING, `offline` when the host lists
//!   the port but nothing answers (mid-boot, wedged, busy installing).
//! - SPACE AVAILABLE: the biggest app an install can take now — the largest free run
//!   of a multi-app board's app region (more needs a compaction or an
//!   uninstall), or a single-app board's whole region.
//!
//! An offline row has `-` where only a PING can answer, so every row has the
//! same fields. A row's serial is the one the device reports; firmware older
//! than `picodroid/2.4` has none and is listed by its port. `-l` adds PORT
//! (the tty, or a simulator's socket), RELEASE and, where the OS says
//! (Linux), USB, the bus position bench scripts map a fleet slot by.

use std::io::ErrorKind;
use std::thread;
use std::time::Duration;

use crate::protocol::{recv_response, send_frame, CMD_PING, STATUS_OK};
use crate::select::{self, usable_serial, PortEntry};
use crate::transport::{self, Link};

use pdb_protocol::greeting::Greeting;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// adb's header line, kept word for word so scripts written for adb read
/// this output too.
pub const HEADER: &str = "List of devices attached";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Answers a PING.
    Device,
    /// Listed by the host, silent.
    Offline,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            State::Device => "device",
            State::Offline => "offline",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A board on a USB CDC port; `usb_path` is its bus position
    /// (`1-8.3.4`) where the OS exposes one.
    Usb { usb_path: Option<String> },
    /// A simulator on a Unix socket.
    Sim,
}

/// What the greeting said, for `-l`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub board: String,
    pub release: String,
    pub max_papk: u32,
    /// The largest free run of the app region, on multi-app firmware.
    pub largest_free: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What `-s` takes to reach this device again.
    pub serial: String,
    /// The port or socket to open.
    pub target: String,
    pub state: State,
    pub kind: Kind,
    /// `None` when offline.
    pub info: Option<Info>,
}

pub fn run(long: bool) {
    println!("{HEADER}");
    for line in format_rows(&scan(), long) {
        println!("{line}");
    }
    println!();
}

/// The column names, in order; `-l` shows all of them.
const COLUMNS: [&str; 7] = [
    "SERIAL",
    "STATE",
    "BOARD",
    "SPACE AVAILABLE",
    "PORT",
    "RELEASE",
    "USB",
];
/// How many of [`COLUMNS`] a plain `devices` shows.
const SHORT_COLUMNS: usize = 4;
/// SPACE AVAILABLE is right-aligned, so its numbers line up by digit.
const SPACE_COLUMN: usize = 3;

/// The column row and one line per device, every column padded to its
/// widest cell with two spaces between, nothing trailing. No devices, no
/// column row.
pub fn format_rows(rows: &[Row], long: bool) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }
    let n = if long { COLUMNS.len() } else { SHORT_COLUMNS };
    let mut table: Vec<Vec<String>> = vec![COLUMNS[..n].iter().map(|c| c.to_string()).collect()];
    table.extend(rows.iter().map(|r| {
        let mut c = cells(r);
        c.truncate(n);
        c
    }));
    let widths: Vec<usize> = (0..n)
        .map(|c| {
            table
                .iter()
                .map(|r| r[c].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    table
        .iter()
        .map(|row| {
            let mut line = String::new();
            for (c, cell) in row.iter().enumerate() {
                let w = widths[c];
                if c == SPACE_COLUMN {
                    line.push_str(&format!("{cell:>w$}  "));
                } else {
                    line.push_str(&format!("{cell:<w$}  "));
                }
            }
            line.trim_end().to_owned()
        })
        .collect()
}

/// One row's cells, one per [`COLUMNS`] entry; `-` where there is nothing to
/// say, so a row always splits into the same fields.
fn cells(r: &Row) -> Vec<String> {
    let dash = || "-".to_string();
    let i = r.info.as_ref();
    let board = i.map(|i| i.board.clone()).filter(|b| !b.is_empty());
    let space = i.map(|i| format!("{} KB", i.largest_free.unwrap_or(i.max_papk) / 1024));
    let release = i.map(|i| i.release.clone()).filter(|v| !v.is_empty());
    let usb = match &r.kind {
        Kind::Usb { usb_path } => usb_path.clone(),
        Kind::Sim => None,
    };
    vec![
        r.serial.clone(),
        r.state.as_str().into(),
        board.unwrap_or_else(dash),
        space.unwrap_or_else(dash),
        r.target.clone(),
        release.unwrap_or_else(dash),
        usb.unwrap_or_else(dash),
    ]
}

/// Every device, then every simulator.
pub fn scan() -> Vec<Row> {
    let mut rows = scan_serial();
    rows.extend(scan_sims());
    rows
}

/// Every port the host lists with the picodroid VID/PID, PINGed in
/// parallel. Ports without that VID/PID are never opened: a debug probe's
/// UART is not ours to write to.
pub fn scan_serial() -> Vec<Row> {
    let ports = select::picodroid_ports();
    thread::scope(|s| {
        let probes: Vec<_> = ports
            .iter()
            .map(|p| s.spawn(move || usb_row(p, probe(&p.name))))
            .collect();
        probes.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

/// A USB row from the port and what its PING said. The serial is the one
/// the greeting gives, else the USB string, else the port name.
fn usb_row(port: &PortEntry, greeting: Option<(String, Info)>) -> Row {
    let (state, greeting_serial, info) = match greeting {
        Some((serial, info)) => (State::Device, Some(serial), Some(info)),
        None => (State::Offline, None, None),
    };
    let serial = greeting_serial
        .filter(|s| usable_serial(s))
        .or_else(|| port.serial.clone())
        .unwrap_or_else(|| port.name.clone());
    Row {
        serial,
        target: port.name.clone(),
        state,
        kind: Kind::Usb {
            usb_path: usb_path(&port.name),
        },
        info,
    }
}

/// Every running simulator: each socket under `transport::sim_socket_dir`
/// that answers a PING. A socket nobody listens on any more — a simulator
/// that was killed — is removed on the way past; one that is busy (an
/// install in progress) times out and is left alone.
pub fn scan_sims() -> Vec<Row> {
    let mut found = Vec::new();
    for path in transport::sim_sockets() {
        let name = path.to_string_lossy().into_owned();
        match transport::open(&name, PROBE_TIMEOUT) {
            Ok(mut link) => {
                if let Some((serial, info)) = probe_over(link.as_mut()) {
                    found.push(Row {
                        serial: if usable_serial(&serial) {
                            serial
                        } else {
                            name.clone()
                        },
                        target: name,
                        state: State::Device,
                        kind: Kind::Sim,
                        info: Some(info),
                    });
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

/// A quick PING on `target`: the greeting's serial and the `-l` items.
pub fn probe(target: &str) -> Option<(String, Info)> {
    let mut link = transport::open(target, PROBE_TIMEOUT).ok()?;
    probe_over(link.as_mut())
}

fn probe_over(link: &mut dyn Link) -> Option<(String, Info)> {
    send_frame(link, CMD_PING, b"").ok()?;
    let (status, payload) = recv_response(link).ok()?;
    if status != STATUS_OK {
        return None;
    }
    let g = Greeting::parse(&payload).ok()?;
    Some((
        g.serial.to_owned(),
        Info {
            board: g.board.to_owned(),
            release: g.framework_map_version.to_owned(),
            max_papk: g.max_papk,
            largest_free: g.apps.map(|a| a.largest_free),
        },
    ))
}

/// The USB bus position of a tty (`/dev/ttyACM3` → `1-8.3.4`), from sysfs.
/// Linux only; other hosts do not expose it, and the row goes without.
#[cfg(target_os = "linux")]
fn usb_path(port: &str) -> Option<String> {
    let name = port.strip_prefix("/dev/")?;
    let iface = std::fs::canonicalize(format!("/sys/class/tty/{name}/device")).ok()?;
    usb_path_of_interface(&iface)
}

#[cfg(not(target_os = "linux"))]
fn usb_path(_port: &str) -> Option<String> {
    None
}

/// `…/usb1/1-8/1-8.3/1-8.3.4/1-8.3.4:1.0` → `1-8.3.4`: the interface's
/// parent is the device, named by its bus position.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn usb_path_of_interface(iface: &std::path::Path) -> Option<String> {
    let leaf = iface.file_name()?.to_str()?;
    let (device, _interface) = leaf.split_once(':')?;
    Some(device.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn multi(board: &str, free_kb: u32) -> Info {
        Info {
            board: board.into(),
            release: "0.37.0".into(),
            max_papk: 9724 * 1024,
            largest_free: Some(free_kb * 1024),
        }
    }

    fn single(board: &str, max_kb: u32) -> Info {
        Info {
            board: board.into(),
            release: "0.37.0".into(),
            max_papk: max_kb * 1024,
            largest_free: None,
        }
    }

    fn port(serial: Option<&str>) -> PortEntry {
        PortEntry {
            name: "/dev/ttyACM3".into(),
            serial: serial.map(Into::into),
        }
    }

    fn row(serial: &str, target: &str, state: State, info: Option<Info>, kind: Kind) -> Row {
        Row {
            serial: serial.into(),
            target: target.into(),
            state,
            kind,
            info,
        }
    }

    fn usb(path: Option<&str>) -> Kind {
        Kind::Usb {
            usb_path: path.map(Into::into),
        }
    }

    fn bench() -> Vec<Row> {
        vec![
            row(
                "E66258881755602D",
                "/dev/ttyACM6",
                State::Device,
                Some(single("testbench_rp2040", 764)),
                usb(Some("1-8.1")),
            ),
            row(
                "emulator-4242",
                "/tmp/picodroid-sim/pdb-4242.sock",
                State::Device,
                Some(multi("host", 1528)),
                Kind::Sim,
            ),
            row(
                "/dev/ttyACM9",
                "/dev/ttyACM9",
                State::Offline,
                None,
                usb(Some("1-8.2")),
            ),
            row(
                "1C05163AC1ACB0AA",
                "/dev/ttyACM3",
                State::Device,
                Some(multi("pico_touch_kit", 9720)),
                usb(Some("1-8.3.4")),
            ),
        ]
    }

    /// Serials and board names of different lengths still line up under
    /// their column names; sizes line up by digit.
    #[test]
    fn columns_line_up_under_their_names() {
        assert_eq!(
            format_rows(&bench(), false),
            [
                "SERIAL            STATE    BOARD             SPACE AVAILABLE",
                "E66258881755602D  device   testbench_rp2040           764 KB",
                "emulator-4242     device   host                      1528 KB",
                "/dev/ttyACM9      offline  -                               -",
                "1C05163AC1ACB0AA  device   pico_touch_kit            9720 KB",
            ]
        );
    }

    #[test]
    fn the_long_form_names_its_extra_columns_too() {
        assert_eq!(
            format_rows(&bench(), true),
            [
                "SERIAL            STATE    BOARD             SPACE AVAILABLE  PORT                              RELEASE  USB",
                "E66258881755602D  device   testbench_rp2040           764 KB  /dev/ttyACM6                      0.37.0   1-8.1",
                "emulator-4242     device   host                      1528 KB  /tmp/picodroid-sim/pdb-4242.sock  0.37.0   -",
                "/dev/ttyACM9      offline  -                               -  /dev/ttyACM9                      -        1-8.2",
                "1C05163AC1ACB0AA  device   pico_touch_kit            9720 KB  /dev/ttyACM3                      0.37.0   1-8.3.4",
            ]
        );
    }

    /// `$1` is the serial and `$2` the state on every device row, and the
    /// column row is the one `$2 == "device"` skips, as a script written
    /// for adb reads them.
    #[test]
    fn every_row_splits_into_the_same_fields() {
        for long in [false, true] {
            let lines = format_rows(&bench(), long);
            // "SPACE AVAILABLE" is two words, as "764 KB" is.
            let fields = lines[0].split_whitespace().count() - 1;
            assert!(!lines[0].split_whitespace().any(|f| f == "device"));
            for line in &lines[1..] {
                let f: Vec<&str> = line.split_whitespace().collect();
                // The space is two words ("764 KB") except for an offline "-".
                let expect = if f[1] == "offline" {
                    fields
                } else {
                    fields + 1
                };
                assert_eq!(f.len(), expect, "{line}");
                assert!(f[1] == "device" || f[1] == "offline", "{line}");
            }
        }
    }

    #[test]
    fn no_devices_no_column_row() {
        assert!(format_rows(&[], false).is_empty());
    }

    #[test]
    fn a_silent_port_is_offline_under_its_usb_serial() {
        let r = usb_row(&port(Some("E6633861A37B9038")), None);
        assert_eq!(r.state, State::Offline);
        assert_eq!(r.serial, "E6633861A37B9038");
        assert_eq!(r.info, None);
    }

    /// Firmware older than 2.4 sends no serial anywhere; the port stands in.
    #[test]
    fn a_board_without_a_serial_is_listed_by_its_port() {
        let r = usb_row(&port(None), Some((String::new(), single("tb", 764))));
        assert_eq!(r.serial, "/dev/ttyACM3");
        let r = usb_row(&port(None), Some(("unknown".into(), single("tb", 764))));
        assert_eq!(r.serial, "/dev/ttyACM3");
        assert_eq!(r.state, State::Device);
    }

    #[test]
    fn the_bus_position_is_the_interfaces_parent() {
        let p = std::path::Path::new("/sys/devices/pci0000:00/usb1/1-8/1-8.3/1-8.3.4/1-8.3.4:1.0");
        assert_eq!(usb_path_of_interface(p).as_deref(), Some("1-8.3.4"));
        assert_eq!(
            usb_path_of_interface(std::path::Path::new("/sys/x/1-8")),
            None
        );
    }
}
