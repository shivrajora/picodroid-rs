// SPDX-License-Identifier: GPL-3.0-only
//! The natives behind `picodroid.net.wifi.WifiManager`, over `hal::wifi`
//! (docs/designs/wifi-provisioning-2026-09.md). Every method is static;
//! `args[0]` is the first Java parameter. Nothing here touches the driver:
//! a join, scan or leave is parked in the mailbox for the link task.

use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

use crate::hal::wifi::{self, Credentials, Request, Security, SSID_MAX};
use crate::shrink_names::m;

pub fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    Some(match method_name {
        m::nativeAvailable => Ok(Some(Value::Int(1))),
        m::nativeStartScan => Ok(Some(bool_value(
            wifi::is_scanning() || wifi::submit(Request::Scan),
        ))),
        m::nativeScanGeneration => Ok(Some(Value::Int(wifi::scan_generation() as i32))),
        m::nativeScanCount => Ok(Some(Value::Int(wifi::scan_count() as i32))),
        m::nativeScanSsid => match entry_arg(ctx) {
            Some(e) => interned(ctx, e.ssid()),
            None => interned(ctx, b""),
        },
        m::nativeScanBssid => {
            let bssid = entry_arg(ctx).map(|e| e.bssid).unwrap_or([0; 6]);
            let mut text = [0u8; 17];
            format_bssid(&bssid, &mut text);
            interned(ctx, &text)
        }
        m::nativeScanRssi => Ok(Some(Value::Int(
            entry_arg(ctx).map_or(-127, |e| i32::from(e.rssi)),
        ))),
        m::nativeScanChannel => Ok(Some(Value::Int(
            entry_arg(ctx).map_or(0, |e| i32::from(e.channel)),
        ))),
        m::nativeScanSecurity => Ok(Some(Value::Int(
            entry_arg(ctx).map_or(0, |e| e.security as i32),
        ))),
        m::nativeSave => {
            let saved = credentials_arg(ctx).is_some_and(|c| wifi::save_stored(&c));
            Ok(Some(bool_value(saved)))
        }
        m::nativeForget => Ok(Some(bool_value(wifi::forget_stored()))),
        m::nativeReconnect => {
            let sent = match wifi::configured().1 {
                Some(c) => wifi::submit(Request::Join(c)),
                None => false,
            };
            Ok(Some(bool_value(sent)))
        }
        m::nativeDisconnect => Ok(Some(bool_value(wifi::submit(Request::Leave)))),
        m::nativeStatus => Ok(Some(Value::Int(wifi::status() as i32))),
        m::nativeCurrentSsid => {
            let mut ssid = [0u8; SSID_MAX];
            let n = wifi::current_ssid(&mut ssid);
            interned(ctx, &ssid[..n])
        }
        m::nativeSavedSsid => match wifi::configured().1 {
            Some(c) => interned(ctx, c.ssid()),
            None => interned(ctx, b""),
        },
        m::nativeSavedSource => Ok(Some(Value::Int(wifi::configured().0 as i32))),
        _ => return None,
    })
}

fn bool_value(b: bool) -> Value {
    Value::Int(b as i32)
}

/// A Java `String` for `bytes`, interned.
fn interned(ctx: &mut NativeContext<'_>, bytes: &[u8]) -> Result<Option<Value>, JvmError> {
    match ctx.strings.intern_dyn(bytes) {
        Some(idx) => Ok(Some(Value::Reference(idx))),
        None => Err(JvmError::StackOverflow),
    }
}

/// The scan entry the `int index` argument names.
fn entry_arg(ctx: &NativeContext<'_>) -> Option<wifi::ScanEntry> {
    match ctx.args.first() {
        Some(Value::Int(i)) if *i >= 0 => wifi::scan_entry(*i as usize),
        _ => None,
    }
}

/// The `String` at `args[i]`, if it is one.
fn string_arg<'a>(ctx: &'a NativeContext<'_>, i: usize) -> Option<&'a str> {
    match ctx.args.get(i) {
        Some(Value::Reference(idx)) => ctx.strings.resolve(*idx),
        _ => None,
    }
}

/// `(String ssid, String password)` as credentials. The security is what
/// the last scan reported for that SSID; for a network never scanned, a
/// password means WPA2/WPA3 and none means open.
fn credentials_arg(ctx: &NativeContext<'_>) -> Option<Credentials> {
    let ssid = string_arg(ctx, 0)?.as_bytes();
    let pass = string_arg(ctx, 1).unwrap_or("").as_bytes();
    let security = wifi::scan_lookup(ssid)
        .map(|e| e.security)
        .unwrap_or_else(|| Security::guess(!pass.is_empty()));
    Credentials::new(ssid, pass, security)
}

/// `aa:bb:cc:dd:ee:ff` into a 17-byte buffer.
fn format_bssid(bssid: &[u8; 6], out: &mut [u8; 17]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, b) in bssid.iter().enumerate() {
        out[i * 3] = HEX[(b >> 4) as usize];
        out[i * 3 + 1] = HEX[(b & 0xF) as usize];
        if i < 5 {
            out[i * 3 + 2] = b':';
        }
    }
}
