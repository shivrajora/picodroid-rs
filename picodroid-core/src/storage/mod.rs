// SPDX-License-Identifier: GPL-3.0-only
//! Per-app storage on the one LittleFS volume
//! (docs/designs/multi-app-2026-09.md D10, §8): the sandbox that confines
//! every app path to `/data/<package>`, and — multi-app M3b — the quota,
//! the wipe an uninstall runs and the boot sweep. All of it reaches storage
//! through `crate::hal::fs`, so it is family-neutral and runs under test
//! over the in-memory `TestHal`.

pub mod sandbox;
