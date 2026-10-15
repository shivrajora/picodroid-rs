// SPDX-License-Identifier: GPL-3.0-only
//! The device installer, end to end over an in-memory NOR region: peek,
//! compat and feature checks, placement, erase, stream, CRC, the deep check
//! of the image now in flash, commit.

use std::cell::RefCell;

use papk_format::flash_image::META_SIZE;
use papk_format::{find_manifest_value, keys, validate_structure, AlignedBuf};
use pd_install::mem_region::{MemRegion, MemTransport, NoCoordinator};
use pd_install::{run_sectors, PackageDirectory, PapkFlash, Plan, PlanError};

use crate::Bytes;

/// 16 sectors: 60 KB of app, more than libFuzzer grows an input to here.
const SECTORS: usize = 16;

/// The one feature this board has.
const PRESENT_FEATURE: &str = "fuzz.hardware.present";

thread_local! {
    // One region for the process: `MemRegion::new` leaks its buffer, as
    // flash is never freed, so a region per input would grow without bound.
    static REGION: RefCell<MemRegion> = RefCell::new(MemRegion::new(SECTORS, 1));
}

/// A single-app board's directory over an empty region: every install goes
/// to sector 0, as the orchestrator's own tests place it. The multi-app
/// placement rules are pinned in picodroid-core's `packages` tests.
struct Directory {
    region_sectors: u32,
}

impl PackageDirectory for Directory {
    const CAN_COMPACT: bool = false;

    fn has_feature(&self, feature: &str) -> bool {
        feature == PRESENT_FEATURE
    }
    fn rescan(&mut self, flash: &impl PapkFlash) {
        self.region_sectors = (flash.region_len() / META_SIZE) as u32;
    }
    fn mark_verified(&mut self, _flash: &mut impl PapkFlash) {}
    fn plan_install(
        &mut self,
        _package: Option<&str>,
        papk_len: usize,
        _max_apps: usize,
    ) -> Result<Plan, PlanError> {
        let need = run_sectors(papk_len);
        if papk_len == 0 || need > self.region_sectors {
            return Err(PlanError::TooLarge);
        }
        Ok(Plan {
            first_sector: 0,
            erase_before: (0, need),
            evict_after: None,
            flags: papk_format::flash_image::FLAG_BOOT_DEFAULT,
            seq: 1,
            compact_first: false,
        })
    }
    fn compact(&mut self, _flash: &mut impl PapkFlash) {}
    fn free_space(&self) -> (u32, u32) {
        (self.region_sectors, self.region_sectors)
    }
    fn installed_count(&self) -> u32 {
        0
    }
}

/// Input: a byte choosing the firmware's map version (unshrunk or shrunk),
/// then the PAPK as the wire carries it, with a correct CRC.
///
/// An install that reports success must have committed exactly the bytes
/// sent, which pass the deep check, carry a map version the firmware is
/// compatible with and require no feature the board lacks. One that
/// refuses must leave the region as it found it: erased.
pub fn install(data: &[u8]) {
    let mut b = Bytes::new(data);
    let fw = if b.u8() & 1 == 0 { "0.0.0" } else { "0.40.0" };
    let papk = b.rest();
    let mut transport = MemTransport::for_papk(papk);
    let mut directory = Directory {
        region_sectors: SECTORS as u32,
    };
    REGION.with_borrow_mut(|region| {
        region.erase_all();
        let ok = pd_install::install(
            &mut transport,
            &mut NoCoordinator,
            region,
            &mut directory,
            fw,
            papk.len() as u32,
        );
        if ok {
            assert!(transport.success && transport.error.is_none());
            let image = &region.bytes()[META_SIZE..META_SIZE + papk.len()];
            assert_eq!(image, papk, "the committed image is not what was sent");
            let image = AlignedBuf::new(image);
            assert!(
                validate_structure(&image).is_ok(),
                "committed an invalid image"
            );
            let fmv = find_manifest_value(&image, keys::FRAMEWORK_MAP_VERSION);
            assert!(
                compat::check(fmv, fw).is_ok(),
                "committed map version {fmv:?} on firmware {fw}"
            );
            if let Some(list) = find_manifest_value(&image, keys::REQUIRES_FEATURES) {
                assert!(
                    list.split(',')
                        .all(|f| f.is_empty() || f == PRESENT_FEATURE),
                    "committed an app that requires {list:?}"
                );
            }
        } else {
            assert!(!transport.success, "reported success and returned false");
            assert!(transport.error.is_some(), "refused without saying why");
            assert!(
                region.bytes().iter().all(|&b| b == 0xFF),
                "a refused install ({:?}) left bytes in the region",
                transport.error
            );
        }
    });
}
