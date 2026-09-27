// SPDX-License-Identifier: GPL-3.0-only
//! The simulator's WiFi: what a device's link driver does with a
//! `hal::wifi` request, faked on the host (docs/designs/wifi-provisioning-2026-09.md).
//!
//! The host's network is always there (`hal/sim/net.rs`), so provisioning
//! is modelled on top of it: a scan answers with a canned list of access
//! points, a join succeeds for a listed SSID with the right password and
//! takes the simulated link up after a short delay, and fails — BADAUTH or
//! NONET — otherwise, taking the link down. Nothing here touches a socket.
//!
//! - `PICODROID_SIM_WIFI_NETWORKS` — the access points a scan finds, as
//!   `ssid:security:rssi` triples separated by commas; security is `open`,
//!   `wpa`, `wpa2`, `wpa3` or `wpa2wpa3`. Default: three networks.
//! - `PICODROID_SIM_WIFI_PASS` — the password every secured one accepts
//!   (default `picodroid`).
//!
//! Requests arrive through [`service`], called from `hal::wifi::submit` on
//! the JVM task; the join's delay runs on a host thread so the screen sees
//! Connecting… first, as it does on a device.

use std::time::Duration;

use crate::hal::wifi::{self, Credentials, Request, ScanEntry, Security, Status};

const DEFAULT_NETWORKS: &str = "picodroid-lab:wpa2:-45,Cafe Guest:open:-70,Neighbour:wpa2wpa3:-82";
const DEFAULT_PASS: &str = "picodroid";
const JOIN_DELAY: Duration = Duration::from_millis(800);

fn security_named(name: &str) -> Security {
    match name.trim().to_ascii_lowercase().as_str() {
        "open" => Security::Open,
        "wep" => Security::Wep,
        "wpa" => Security::Wpa,
        "wpa3" => Security::Wpa3,
        "wpa2wpa3" | "wpa3wpa2" => Security::Wpa2Wpa3,
        _ => Security::Wpa2,
    }
}

/// The canned access points: one scan entry per `ssid:security:rssi`.
fn networks() -> Vec<ScanEntry> {
    let spec =
        std::env::var("PICODROID_SIM_WIFI_NETWORKS").unwrap_or_else(|_| DEFAULT_NETWORKS.into());
    spec.split(',')
        .enumerate()
        .filter_map(|(i, item)| {
            let mut parts = item.splitn(3, ':');
            let ssid = parts.next()?.trim();
            let security = security_named(parts.next().unwrap_or("wpa2"));
            let rssi: i16 = parts
                .next()
                .and_then(|r| r.trim().parse().ok())
                .unwrap_or(-60);
            let bssid = [0x02, 0x00, 0x00, 0x00, 0x00, i as u8 + 1];
            ScanEntry::new(
                ssid.as_bytes(),
                bssid,
                rssi,
                1 + (i as u8 * 5) % 11,
                security,
            )
        })
        .collect()
}

fn accepted_password() -> String {
    std::env::var("PICODROID_SIM_WIFI_PASS").unwrap_or_else(|_| DEFAULT_PASS.into())
}

/// Boot: join the configured network as a device's `bring_up` would.
pub fn boot() {
    let (source, creds) = wifi::configured();
    match creds {
        Some(c) => {
            println!(
                "[sim] wifi: joining \"{}\" ({})",
                String::from_utf8_lossy(c.ssid()),
                match source {
                    wifi::Source::Build => "build",
                    _ => "stored",
                }
            );
            join(&c);
        }
        None => println!("[sim] wifi: no network configured — not joining"),
    }
}

/// Run whatever the mailbox holds. Called by `hal::wifi::submit` at once.
pub fn service() {
    while let Some(req) = wifi::take_request() {
        match req {
            Request::Scan => scan(),
            Request::Join(c) => join(&c),
            Request::Leave => leave(),
        }
    }
}

fn scan() {
    wifi::scan_begin();
    let list = networks();
    for e in &list {
        wifi::scan_add(*e);
    }
    wifi::scan_end();
    println!("[sim] wifi: scan found {} networks", list.len());
}

fn join(c: &Credentials) {
    let ssid = String::from_utf8_lossy(c.ssid()).into_owned();
    wifi::set_current_ssid(c.ssid());
    wifi::set_status(Status::Joining);
    let known = networks().into_iter().find(|e| e.ssid() == c.ssid());
    let verdict = match known {
        None => Status::NoNet,
        Some(ap) if !ap.security.secured() => Status::Joined,
        Some(ap) if ap.security == Security::Wep => Status::Fail,
        Some(_) if c.pass() == accepted_password().as_bytes() => Status::Joined,
        Some(_) => Status::BadAuth,
    };
    // The verdict lands after a delay, as the driver's events would, on a
    // host thread outside the modelled heap.
    let _spawn_bypass = crate::host::heap_bypass();
    std::thread::spawn(move || {
        std::thread::sleep(JOIN_DELAY);
        match verdict {
            Status::Joined => {
                println!("[sim] wifi: joined \"{ssid}\"");
                wifi::set_status(Status::Joined);
                super::net::set_link_up(true);
            }
            other => {
                println!("[sim] wifi: join \"{ssid}\" failed: {other:?}");
                super::net::set_link_up(false);
                wifi::set_status(other);
            }
        }
    });
}

fn leave() {
    println!("[sim] wifi: left");
    super::net::set_link_up(false);
    wifi::set_status(Status::Down);
}
