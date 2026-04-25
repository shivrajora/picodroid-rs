# compat

Picodroid PAPK ↔ firmware `framework-map-version` compatibility check.

A single `no_std` crate that owns the rule for whether a given PAPK can be loaded by a given firmware build. The same code runs in two places:

- **Device** — `pico-jvm`'s `Papk::verify_compat` calls into this crate at PAPK load time so an incompatible PAPK is rejected before any class is loaded.
- **Host** — `tools/pdb`'s `install` pre-flight calls into the same crate so the device never reboots into an incompatible image over USB.

Keeping the rule here prevents the two paths from drifting as the shrink-map design evolves.

## Usage

```toml
[dependencies]
compat = { path = "../compat" }
```

```rust
use compat::{check, CompatError};

let papk_version = Some("0.1.0");      // from PAPK manifest
let firmware_version = "0.1.0";         // from build.rs

match check(papk_version, firmware_version) {
    Ok(()) => { /* load PAPK */ }
    Err(CompatError::Mismatch) => { /* refuse — versions are incompatible */ }
    Err(CompatError::Missing)  => { /* refuse — legacy PAPK on shrunk firmware */ }
    Err(CompatError::BadVersion) => { /* refuse — malformed semver */ }
}
```

See `docs/shrinker.md` in the parent repo for the full design.

## Status

Internal to the picodroid-rs project. Not published to crates.io.

## License

Apache-2.0
