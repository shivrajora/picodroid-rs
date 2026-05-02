// SPDX-License-Identifier: GPL-3.0-only
//! Bundled-image-asset registry.
//!
//! At app load (after the JVM has finished class loading) [`init_from_papk`]
//! walks the papk's `ASST` section and builds a `name → *const lv_image_dsc_t`
//! lookup. Each descriptor is heap-`Box`-leaked once so its address stays
//! stable for the lifetime of the firmware; LVGL keeps the raw pointer it
//! receives via `lv_image_set_src`, so we can't move the descriptor out from
//! under it.
//!
//! The descriptor's `data` field points directly into the XIP-mapped papk
//! slice (which itself is `'static` because the papk lives in flash), so we
//! never copy pixel bytes into RAM. RAM cost per asset is one
//! `lv_image_dsc_t` (24 bytes on 32-bit) plus the heap-allocated name.
//!
//! Same single-core safety story as [`crate::system::monitor_store`]: all
//! lookups happen on the JVM task and the registry is built once before any
//! lookup can run.

use crate::lvgl_ffi::{lv_image_dsc_t, lv_image_header_t};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use pico_jvm::apk::Papk;

struct RegistryCell {
    inner: UnsafeCell<Registry>,
}

struct Registry {
    initialized: bool,
    entries: Vec<(String, *const lv_image_dsc_t)>,
}

// SAFETY: single-core JVM task ownership, mirrors `monitor_store::MonitorStoreCell`.
// `init_from_papk` runs once at app start before any lookup can happen, so the
// `&mut` it takes during init does not race with `lookup`'s `&` reads.
unsafe impl Sync for RegistryCell {}

static REGISTRY: RegistryCell = RegistryCell {
    inner: UnsafeCell::new(Registry {
        initialized: false,
        entries: Vec::new(),
    }),
};

fn registry() -> &'static mut Registry {
    unsafe { &mut *REGISTRY.inner.get() }
}

/// Walk the papk's ASSETS section and build the lookup table.
///
/// Idempotent: a second call is a no-op so callers don't have to track init
/// state. Safe to call when the papk has no ASSETS section — the registry
/// stays empty.
///
/// # Safety
///
/// The caller must guarantee that the papk slice is reachable for the
/// firmware's lifetime: either XIP-mapped flash (the `read_flash_papk` path)
/// or the static `pdb` receive buffer. We extract raw pointers into the
/// pixel data and hand them to LVGL, which holds them indefinitely.
pub fn init_from_papk(papk: &Papk<'_>) {
    let reg = registry();
    if reg.initialized {
        return;
    }
    reg.initialized = true;
    let assets_iter = match papk.assets() {
        Ok(Some(it)) => it,
        Ok(None) | Err(_) => return,
    };
    for entry in assets_iter {
        let Ok(name) = core::str::from_utf8(entry.name) else {
            continue;
        };
        let header = lv_image_header_t::new(entry.cf, entry.width, entry.height, entry.stride);
        let dsc = Box::new(lv_image_dsc_t {
            header,
            data_size: entry.data.len() as u32,
            data: entry.data.as_ptr(),
            reserved: core::ptr::null(),
            reserved_2: core::ptr::null(),
        });
        let dsc_ptr: *const lv_image_dsc_t = Box::leak(dsc);
        reg.entries.push((name.to_string(), dsc_ptr));
    }
    // One-line breadcrumb so HIL/sim tests can assert the section parsed.
    // Skipped when no assets exist — silent path stays silent.
    if !reg.entries.is_empty() {
        #[cfg(not(feature = "sim"))]
        defmt::info!("[assets] {=usize} images loaded", reg.entries.len());
        #[cfg(feature = "sim")]
        println!("[assets] {} images loaded", reg.entries.len());
    }
}

/// Look up a bundled asset by exact name (e.g. `"logo.png"`).
///
/// Returns the LVGL descriptor pointer suitable for `lv_image_set_src`.
/// `None` means the name isn't in the bundle — callers typically log a
/// warning and leave the image widget empty.
pub fn lookup(name: &str) -> Option<*const lv_image_dsc_t> {
    registry()
        .entries
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, ptr)| *ptr)
}

/// Number of registered assets — exposed for sim smoke tests and debug
/// output. Allowed to be dead in production builds; exists so app code can
/// log a startup line that confirms the bundle parsed.
#[allow(dead_code)]
pub fn count() -> usize {
    registry().entries.len()
}

/// Drop all asset descriptors. Called on app reset before running a new APK
/// (mirrors `monitor_store::clear`). The pointers we leaked stay leaked —
/// the next init will allocate fresh descriptors. This is acceptable because
/// `pdb install` is a developer flow, not a steady-state occurrence.
pub fn clear() {
    let reg = registry();
    reg.entries.clear();
    reg.initialized = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        clear();
    }

    fn build_minimal_v10_papk() -> Vec<u8> {
        let mut file: Vec<u8> = Vec::new();
        file.extend_from_slice(b"PAPK");
        file.extend_from_slice(&1u16.to_le_bytes()); // version_major
        file.extend_from_slice(&0u16.to_le_bytes()); // version_minor
        file.extend_from_slice(&2u32.to_le_bytes()); // section_count
        file.extend_from_slice(&24u32.to_le_bytes()); // manifest_offset
        file.extend_from_slice(&40u32.to_le_bytes()); // classes_offset
        file.extend_from_slice(&0u32.to_le_bytes()); // assets_offset (none)
        file.extend_from_slice(&u32::from_le_bytes(*b"MANI").to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&u32::from_le_bytes(*b"CLSS").to_le_bytes());
        file.extend_from_slice(&4u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file
    }

    #[test]
    fn init_is_idempotent_on_legacy_papk() {
        reset();
        let papk_bytes = build_minimal_v10_papk();
        let leaked: &'static [u8] = Box::leak(papk_bytes.into_boxed_slice());
        let papk = Papk::parse(leaked).unwrap();
        init_from_papk(&papk);
        init_from_papk(&papk);
        assert_eq!(count(), 0);
        assert!(lookup("anything").is_none());
    }
}
