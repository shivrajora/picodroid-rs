// SPDX-License-Identifier: GPL-3.0-only
//! What this board has, by the names `PackageManager.hasSystemFeature`
//! answers to and a manifest's `<uses-feature>` declares
//! (docs/designs/app-portability-2026-10.md D9). One table, read by the
//! Java query and by the installer's gate, so an app that asks and an app
//! that insists are judged alike.

/// `PackageManager.FEATURE_WIFI`.
pub const WIFI: &str = "picodroid.hardware.wifi";
/// `PackageManager.FEATURE_ETHERNET`.
pub const ETHERNET: &str = "picodroid.hardware.ethernet";
/// `PackageManager.FEATURE_TOUCHSCREEN`.
pub const TOUCHSCREEN: &str = "picodroid.hardware.touchscreen";

/// Whether this board has `feature`. Unknown names are not features it has.
pub fn has(feature: &str) -> bool {
    match feature {
        // The link kind, a build fact (board_cfg.rs emits
        // network_link_<kind> from board.toml's network_type).
        WIFI => cfg!(network_link_wifi),
        ETHERNET => cfg!(network_link_ethernet),
        // A `[touch]` panel in board.toml.
        TOUCHSCREEN => cfg!(has_touch),
        _ => false,
    }
}
