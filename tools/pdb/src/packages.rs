// SPDX-License-Identifier: GPL-3.0-only
//! `pdb list` and `pdb uninstall <package>` — the package directory of a
//! multi-app device.

use std::process;
use std::time::Duration;

use crate::install::{query_device, wait_for_reboot};
use crate::protocol::{
    recv_response, send_frame, status_str, CMD_LIST, CMD_UNINSTALL, STATUS_ERR, STATUS_NOT_FOUND,
    STATUS_OK,
};

const BAUD_RATE: u32 = 115_200;
const TIMEOUT: Duration = Duration::from_secs(5);
/// An uninstall erases a whole run before answering — a 1.5 MB region at
/// ~50 ms a sector is under 20 s.
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(60);

/// One row of the device's `CMD_LIST` answer.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub sector: u32,
    pub package: String,
    pub version_code: u32,
    pub version: String,
    pub size: u32,
    pub boot: bool,
    pub label: String,
}

/// The `free` footer: bytes and counts.
#[derive(Debug, PartialEq, Eq)]
pub struct Free {
    pub largest: u32,
    pub total: u32,
    pub installed: u32,
    pub max: u32,
}

/// Parse the device's tab-separated text. Unknown or short rows are skipped
/// rather than fatal, so a newer firmware can add columns.
pub fn parse_list(text: &str) -> (Vec<Row>, Option<Free>) {
    let mut rows = Vec::new();
    let mut free = None;
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.first() == Some(&"free") && f.len() >= 5 {
            free = Some(Free {
                largest: f[1].parse().unwrap_or(0),
                total: f[2].parse().unwrap_or(0),
                installed: f[3].parse().unwrap_or(0),
                max: f[4].parse().unwrap_or(0),
            });
            continue;
        }
        if f.len() < 7 {
            continue;
        }
        rows.push(Row {
            sector: f[0].parse().unwrap_or(0),
            package: f[1].to_string(),
            version_code: f[2].parse().unwrap_or(0),
            version: f[3].to_string(),
            size: f[4].parse().unwrap_or(0),
            boot: f[5] == "boot",
            label: f[6..].join("\t"),
        });
    }
    (rows, free)
}

pub fn render(rows: &[Row], free: Option<&Free>) -> String {
    let mut out = String::new();
    if rows.is_empty() {
        out.push_str("(no apps installed)\n");
    } else {
        let w_pkg = rows
            .iter()
            .map(|r| r.package.len())
            .max()
            .unwrap_or(7)
            .max(7);
        let w_ver = rows
            .iter()
            .map(|r| r.version.len())
            .max()
            .unwrap_or(7)
            .max(7);
        out.push_str(&format!(
            "{:<6} {:<w_pkg$} {:>5} {:<w_ver$} {:>8}  {:<4} {}\n",
            "SECTOR", "PACKAGE", "CODE", "VERSION", "SIZE", "BOOT", "LABEL"
        ));
        for r in rows {
            out.push_str(&format!(
                "{:<6} {:<w_pkg$} {:>5} {:<w_ver$} {:>5} KB  {:<4} {}\n",
                r.sector,
                r.package,
                r.version_code,
                r.version,
                r.size.div_ceil(1024),
                if r.boot { "yes" } else { "-" },
                r.label
            ));
        }
    }
    if let Some(f) = free {
        out.push_str(&format!(
            "free: largest {} KB, total {} KB, apps {}/{}\n",
            f.largest / 1024,
            f.total / 1024,
            f.installed,
            f.max
        ));
    }
    out
}

fn open(port_name: &str, timeout: Duration) -> Box<dyn serialport::SerialPort> {
    match serialport::new(port_name, BAUD_RATE)
        .timeout(timeout)
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot open {port_name}: {e}");
            process::exit(1);
        }
    }
}

/// `pdb list`.
pub fn list(port_name: &str) {
    let device = query_device(port_name, TIMEOUT).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        process::exit(1);
    });
    if device.apps.is_none() {
        eprintln!(
            "error: {} is single-app firmware (or predates picodroid/2.2): no package directory to list",
            device.version
        );
        process::exit(1);
    }
    let mut port = open(port_name, TIMEOUT);
    if let Err(e) = send_frame(port.as_mut(), CMD_LIST, b"") {
        eprintln!("error: LIST send failed: {e}");
        process::exit(1);
    }
    match recv_response(port.as_mut()) {
        Ok((STATUS_OK, payload)) => {
            let text = String::from_utf8_lossy(&payload);
            let (rows, free) = parse_list(&text);
            print!("{}", render(&rows, free.as_ref()));
        }
        Ok((status, payload)) => {
            eprintln!(
                "error: LIST returned {} — {}",
                status_str(status),
                String::from_utf8_lossy(&payload)
            );
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: LIST recv failed: {e}");
            process::exit(1);
        }
    }
}

/// `pdb uninstall <package>`.
pub fn uninstall(port_name: &str, package: &str) {
    let device = query_device(port_name, TIMEOUT).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        process::exit(1);
    });
    if device.apps.is_none() {
        eprintln!(
            "error: {} is single-app firmware (or predates picodroid/2.2): install over it instead",
            device.version
        );
        process::exit(1);
    }
    if package.len() > pdb_protocol::MAX_PACKAGE_NAME {
        eprintln!(
            "error: package name longer than {} bytes",
            pdb_protocol::MAX_PACKAGE_NAME
        );
        process::exit(1);
    }
    let mut port = open(port_name, UNINSTALL_TIMEOUT);
    if let Err(e) = send_frame(port.as_mut(), CMD_UNINSTALL, package.as_bytes()) {
        eprintln!("error: UNINSTALL send failed: {e}");
        process::exit(1);
    }
    println!("Uninstalling {package}...");
    match recv_response(port.as_mut()) {
        Ok((STATUS_OK, _)) => {}
        Ok((STATUS_NOT_FOUND, _)) => {
            eprintln!("error: {package} is not installed (pdb list shows what is)");
            process::exit(1);
        }
        Ok((STATUS_ERR, payload)) => {
            eprintln!(
                "error: device refused: {}",
                String::from_utf8_lossy(&payload)
            );
            process::exit(1);
        }
        Ok((status, payload)) => {
            eprintln!(
                "error: UNINSTALL returned {} — {}",
                status_str(status),
                String::from_utf8_lossy(&payload)
            );
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: UNINSTALL recv failed: {e}");
            process::exit(1);
        }
    }
    println!("Uninstalled {package}. Waiting for device to reboot...");
    drop(port);
    if wait_for_reboot(port_name) {
        println!("Device is back.");
    } else {
        eprintln!("warning: device did not respond to PING within 20 s after reboot.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_text_parses_into_rows_and_the_footer() {
        let text = "0\tcom.a\t7\t2.1\t16288\tboot\tApp A\n\
                    6\tcom.b\t1\t1.0\t900\t-\tcom.b\n\
                    free\t1048576\t1441792\t2\t8\n";
        let (rows, free) = parse_list(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].package, "com.a");
        assert!(rows[0].boot);
        assert_eq!(rows[0].label, "App A");
        assert_eq!(rows[1].sector, 6);
        assert!(!rows[1].boot);
        assert_eq!(
            free,
            Some(Free {
                largest: 1048576,
                total: 1441792,
                installed: 2,
                max: 8
            })
        );
        let out = render(&rows, free.as_ref());
        assert!(out.contains("com.a"), "{out}");
        assert!(
            out.contains("free: largest 1024 KB, total 1408 KB, apps 2/8"),
            "{out}"
        );
    }

    #[test]
    fn an_empty_directory_renders_as_such() {
        let (rows, free) = parse_list("free\t0\t0\t0\t8\n");
        assert!(rows.is_empty());
        assert!(render(&rows, free.as_ref()).starts_with("(no apps installed)"));
    }
}
