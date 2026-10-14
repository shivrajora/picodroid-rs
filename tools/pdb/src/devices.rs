// SPDX-License-Identifier: GPL-3.0-only
//! `pdb devices [-l]` — every picodroid on a USB port and every simulator on
//! a socket, one row each, in adb's format: a `List of devices attached`
//! header, then `<serial>\t<state>`.
//!
//! The state is `device` when it answers a PING and `offline` when the host
//! lists the port but nothing answers (mid-boot, wedged, busy installing). A
//! row's serial is the one the device reports; firmware older than
//! `picodroid/2.4` has none and is listed by its port. `-l` adds
//! `port:`/`socket:`, `board:`, `release:`, `max_papk:` and, where the OS says
//! (Linux), `usb:<bus position>` — what bench scripts map a fleet slot to a
//! serial with.

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
    for r in scan() {
        println!("{}", format_row(&r, long));
    }
    println!();
}

/// One row: `<serial>\t<state>`, and with `long` the `key:value` items.
pub fn format_row(r: &Row, long: bool) -> String {
    let mut line = format!("{}\t{}", r.serial, r.state.as_str());
    if !long {
        return line;
    }
    match &r.kind {
        Kind::Usb { .. } => line.push_str(&format!(" port:{}", r.target)),
        Kind::Sim => line.push_str(&format!(" socket:{}", r.target)),
    }
    if let Some(i) = &r.info {
        if !i.board.is_empty() {
            line.push_str(&format!(" board:{}", i.board));
        }
        if !i.release.is_empty() {
            line.push_str(&format!(" release:{}", i.release));
        }
        line.push_str(&format!(" max_papk:{}KB", i.max_papk / 1024));
    }
    if let Kind::Usb { usb_path: Some(p) } = &r.kind {
        line.push_str(&format!(" usb:{p}"));
    }
    line
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

    fn info() -> Info {
        Info {
            board: "pico_touch_kit".into(),
            release: "0.37.0".into(),
            max_papk: 9724 * 1024,
        }
    }

    fn port(serial: Option<&str>) -> PortEntry {
        PortEntry {
            name: "/dev/ttyACM3".into(),
            serial: serial.map(Into::into),
        }
    }

    #[test]
    fn a_row_is_the_serial_a_tab_and_the_state() {
        let r = usb_row(
            &port(Some("E6633861A37B9038")),
            Some(("E6633861A37B9038".into(), info())),
        );
        assert_eq!(format_row(&r, false), "E6633861A37B9038\tdevice");
    }

    #[test]
    fn the_long_form_adds_adbs_key_value_items() {
        let mut r = usb_row(
            &port(Some("E6633861A37B9038")),
            Some(("E6633861A37B9038".into(), info())),
        );
        r.kind = Kind::Usb {
            usb_path: Some("1-8.3.4".into()),
        };
        assert_eq!(
            format_row(&r, true),
            "E6633861A37B9038\tdevice port:/dev/ttyACM3 board:pico_touch_kit \
             release:0.37.0 max_papk:9724KB usb:1-8.3.4"
        );
    }

    #[test]
    fn a_silent_port_is_offline_under_its_usb_serial() {
        let mut r = usb_row(&port(Some("E6633861A37B9038")), None);
        r.kind = Kind::Usb { usb_path: None };
        assert_eq!(r.state, State::Offline);
        assert_eq!(
            format_row(&r, true),
            "E6633861A37B9038\toffline port:/dev/ttyACM3"
        );
    }

    /// Firmware older than 2.4 sends no serial anywhere; the port stands in.
    #[test]
    fn a_board_without_a_serial_is_listed_by_its_port() {
        let r = usb_row(&port(None), Some((String::new(), info())));
        assert_eq!(r.serial, "/dev/ttyACM3");
        let r = usb_row(&port(None), Some(("unknown".into(), info())));
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
