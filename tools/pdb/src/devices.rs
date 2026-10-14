// SPDX-License-Identifier: GPL-3.0-only
//! `pdb devices [-l]` — every picodroid on a USB port and every simulator on
//! a socket, one row each, under adb's `List of devices attached` header:
//! serial, state, board and the app space left, in space-padded columns.
//!
//! adb stops at `<serial>\t<state>`; the board and the space are ours, kept
//! because they are what a bench of several boards is told apart by at a
//! glance. The first two columns are still the serial and the state, so a
//! script's `$1`/`$2` reads the same as for adb.
//!
//! The state is `device` when it answers a PING and `offline` when the host
//! lists the port but nothing answers (mid-boot, wedged, busy installing); an
//! offline row has `-` for the columns only a PING can fill, so every row has
//! the same fields. A row's serial is the one the device reports; firmware
//! older than `picodroid/2.4` has none and is listed by its port. The space
//! is the largest free run of a multi-app board's app region (`free`), or a
//! single-app board's whole region, which an install replaces (`max`). `-l`
//! adds `port:`/`socket:`, `release:` and, where the OS says (Linux),
//! `usb:<bus position>` — what bench scripts map a fleet slot to a serial
//! with.

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

/// The size column, right-aligned so the numbers line up by digit.
const SIZE_COLUMN: usize = 3;

/// The rows as aligned lines: each column padded to its widest cell (the
/// size column on the left, and one space from its unit), two spaces
/// between the rest, nothing trailing.
pub fn format_rows(rows: &[Row], long: bool) -> Vec<String> {
    let cells: Vec<Vec<String>> = rows.iter().map(|r| cells(r, long)).collect();
    let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            cells
                .iter()
                .filter_map(|r| r.get(c))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    cells
        .iter()
        .map(|row| {
            let mut line = String::new();
            for (c, cell) in row.iter().enumerate() {
                let w = widths[c];
                if c == SIZE_COLUMN {
                    line.push_str(&format!("{cell:>w$} "));
                } else {
                    line.push_str(&format!("{cell:<w$}  "));
                }
            }
            line.trim_end().to_owned()
        })
        .collect()
}

/// One row's cells: serial, state, board, the space in KB and what it
/// is; with `long`, the `key:value` items after them.
fn cells(r: &Row, long: bool) -> Vec<String> {
    let (board, kb, what) = match &r.info {
        Some(i) => (
            if i.board.is_empty() {
                "-".into()
            } else {
                i.board.clone()
            },
            i.largest_free.unwrap_or(i.max_papk) / 1024,
            if i.largest_free.is_some() {
                "KB free"
            } else {
                "KB max"
            },
        ),
        None => ("-".into(), 0, ""),
    };
    let size = if r.info.is_some() {
        kb.to_string()
    } else {
        "-".into()
    };
    let mut cells = vec![
        r.serial.clone(),
        r.state.as_str().into(),
        board,
        size,
        what.into(),
    ];
    if long {
        cells.push(match &r.kind {
            Kind::Usb { .. } => format!("port:{}", r.target),
            Kind::Sim => format!("socket:{}", r.target),
        });
        if let Some(i) = r.info.as_ref().filter(|i| !i.release.is_empty()) {
            cells.push(format!("release:{}", i.release));
        }
        if let Kind::Usb { usb_path: Some(p) } = &r.kind {
            cells.push(format!("usb:{p}"));
        }
    }
    cells
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

    fn info(board: &str, largest_free: Option<u32>) -> Info {
        Info {
            board: board.into(),
            release: "0.37.0".into(),
            max_papk: 9724 * 1024,
            largest_free,
        }
    }

    fn port(serial: Option<&str>) -> PortEntry {
        PortEntry {
            name: "/dev/ttyACM3".into(),
            serial: serial.map(Into::into),
        }
    }

    fn row(serial: &str, state: State, info: Option<Info>, kind: Kind) -> Row {
        Row {
            serial: serial.into(),
            target: "/dev/ttyACM3".into(),
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

    /// Serials and board names of different lengths still line up: every
    /// column starts at the same offset on every row.
    #[test]
    fn columns_line_up_across_rows_of_different_widths() {
        let rows = [
            row(
                "E66258881755602D",
                State::Device,
                Some(Info {
                    largest_free: None,
                    max_papk: 764 * 1024,
                    ..info("testbench_rp2040", None)
                }),
                usb(None),
            ),
            row(
                "emulator-4242",
                State::Device,
                Some(info("host", Some(1528 * 1024))),
                Kind::Sim,
            ),
            row("/dev/ttyACM9", State::Offline, None, usb(None)),
            row(
                "1C05163AC1ACB0AA",
                State::Device,
                Some(info("pico_touch_kit", Some(9720 * 1024))),
                usb(None),
            ),
        ];
        assert_eq!(
            format_rows(&rows, false),
            [
                "E66258881755602D  device   testbench_rp2040   764 KB max",
                "emulator-4242     device   host              1528 KB free",
                "/dev/ttyACM9      offline  -                    -",
                "1C05163AC1ACB0AA  device   pico_touch_kit    9720 KB free",
            ]
        );
    }

    /// `$1` is the serial and `$2` the state on every row, offline ones
    /// included, as scripts written for adb read them.
    #[test]
    fn every_row_splits_into_the_same_fields() {
        let rows = [
            row(
                "A",
                State::Device,
                Some(info("tb", Some(4096))),
                usb(Some("1-8.1")),
            ),
            row("B", State::Offline, None, usb(Some("1-8.2"))),
        ];
        for line in format_rows(&rows, false) {
            let f: Vec<&str> = line.split_whitespace().collect();
            assert!(f.len() >= 4, "{line}");
            assert!(f[1] == "device" || f[1] == "offline", "{line}");
        }
    }

    #[test]
    fn the_long_form_adds_key_value_items_after_the_columns() {
        let rows = [row(
            "1C05163AC1ACB0AA",
            State::Device,
            Some(info("pico_touch_kit", Some(9720 * 1024))),
            usb(Some("1-8.3.4")),
        )];
        assert_eq!(
            format_rows(&rows, true),
            [
                "1C05163AC1ACB0AA  device  pico_touch_kit  9720 KB free  port:/dev/ttyACM3  \
              release:0.37.0  usb:1-8.3.4"
            ]
        );
    }

    #[test]
    fn a_silent_port_is_offline_under_its_usb_serial() {
        let mut r = usb_row(&port(Some("E6633861A37B9038")), None);
        r.kind = usb(None);
        assert_eq!(r.state, State::Offline);
        assert_eq!(
            format_rows(&[r], true),
            ["E6633861A37B9038  offline  -  -   port:/dev/ttyACM3"]
        );
    }

    /// Firmware older than 2.4 sends no serial anywhere; the port stands in.
    #[test]
    fn a_board_without_a_serial_is_listed_by_its_port() {
        let r = usb_row(&port(None), Some((String::new(), info("tb", None))));
        assert_eq!(r.serial, "/dev/ttyACM3");
        let r = usb_row(&port(None), Some(("unknown".into(), info("tb", None))));
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
