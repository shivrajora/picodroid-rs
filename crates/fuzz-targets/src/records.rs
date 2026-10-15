// SPDX-License-Identifier: GPL-3.0-only
//! The small fixed-layout records: the compat rule's version strings (from
//! a PAPK's manifest), the boot-meta sector in front of each installed run,
//! and a RESOURCES table on its own.

use papk_format::flash_image;
use papk_format::res::ResTable;

use crate::Bytes;

/// Input: a selector byte, then the record.
pub fn records(data: &[u8]) {
    let mut b = Bytes::new(data);
    match b.u8() % 3 {
        0 => versions(&mut b),
        1 => {
            let sector = b.rest();
            let max = sector.len();
            if let Some(m) = flash_image::parse_meta(sector, max) {
                let _ = m;
                assert!(flash_image::parse_header(sector, max).is_some());
            }
        }
        _ => {
            if let Ok(t) = ResTable::parse(b.rest()) {
                for (ty, n) in t.types() {
                    for e in 0..n.min(64) {
                        let id = papk_format::res::res_id(ty, e);
                        let _ = (t.value(id), t.string(id), t.layout(id), t.style(id));
                    }
                }
                let _ = t.overrides().take(16).count();
            }
        }
    }
}

/// The compat rule is a partial order: a version is compatible with
/// itself, and the sentinel `0.0.0` only with itself.
fn versions(b: &mut Bytes<'_>) {
    let papk = String::from_utf8_lossy(b.take(24)).into_owned();
    let fw = String::from_utf8_lossy(b.rest()).into_owned();
    let _ = compat::check(Some(&papk), &fw);
    let _ = compat::check(None, &fw);
    if let Some(v) = compat::parse_semver(&fw) {
        assert!(
            compat::check(Some(&fw), &fw).is_ok(),
            "{fw:?} refuses itself"
        );
        let zero = v == (0, 0, 0);
        assert_eq!(
            compat::check(None, &fw).is_ok(),
            zero,
            "{fw:?} vs a legacy PAPK"
        );
    }
}
