// SPDX-License-Identifier: GPL-3.0-only
//! `CMD_LIST`'s rows — the package directory over the bridge
//! (docs/designs/multi-app-2026-09.md D8). Multi-app boards only; the frame
//! handling (`pd_pdb::packages`) answers `unknown cmd` elsewhere.
//!
//! The list is text, one tab-separated row per installed app, one per
//! system app, a `free` footer and a `running` line, so `tools/pdb` renders
//! it without a second wire format and a person can read the raw frame in
//! a serial capture. Rows only ever get added (multi-app M2 added the
//! system rows and the running line), so an older host still parses it.

use core::fmt::Write;

use crate::packages::{self, Kind, SECTOR};

/// Rows `sector<TAB>package<TAB>version-code<TAB>version<TAB>size<TAB>flags
/// <TAB>label`, in sector order, then the system apps with `system` in the
/// sector column, then `free<TAB>largest<TAB>total<TAB>installed<TAB>max`
/// (bytes and counts), then `running<TAB>package` when an app is running.
pub fn render_list(text: &mut dyn Write) {
    // Sector order without allocating: repeatedly take the lowest sector
    // above the last one printed. A few dozen entries at the very most.
    let mut last: Option<u32> = None;
    loop {
        let next = packages::apps()
            .filter(|e| last.is_none_or(|l| (e.first_sector as u32) > l))
            .min_by_key(|e| e.first_sector);
        let Some(e) = next else { break };
        let _ = writeln!(
            text,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            e.first_sector,
            e.package(),
            e.version_code(),
            e.version(),
            e.size(),
            if e.is_boot_default() { "boot" } else { "-" },
            e.label()
        );
        last = Some(e.first_sector as u32);
    }
    for e in packages::entries().filter(|e| e.kind == Kind::System) {
        let _ = writeln!(
            text,
            "system\t{}\t{}\t{}\t{}\t-\t{}",
            e.package(),
            e.version_code(),
            e.version(),
            e.size(),
            e.label()
        );
    }
    let (largest, total) = packages::free_space();
    let _ = writeln!(
        text,
        "free\t{}\t{}\t{}\t{}",
        largest as usize * SECTOR,
        total as usize * SECTOR,
        packages::installed_count(),
        packages::MAX_INSTALLED_APPS
    );
    if let Some(running) = packages::running() {
        let _ = writeln!(text, "running\t{running}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::mem_region::{MemRegion, MemTransport, NoCoordinator};
    use crate::install::{install, CoreDirectory};
    use papk_format::{EntryPoint, ManifestSpec, PapkBuilder};
    use pd_pdb::packages::{handle_list, handle_uninstall};
    use pd_pdb::vec_pipe::VecPipe;
    use pdb_protocol::{crc32_frame, CMD_LIST, CMD_UNINSTALL, STATUS_NOT_FOUND, STATUS_OK};

    fn papk(package: &str, label: Option<&str>) -> Vec<u8> {
        let mut b = PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("t/Main"),
            package_name: package,
            version: "2.1",
            framework_map_version: crate::framework_map::FRAMEWORK_MAP_VERSION,
            version_code: Some(7),
            label,
            icon: None,
            design_size: None,
            requires_features: None,
        });
        let main = crate::packages::test_support::t_main();
        b.class("t/Main", &main);
        b.build().unwrap()
    }

    fn seed(region: &mut MemRegion, bytes: &[u8]) {
        let mut t = MemTransport::for_papk(bytes);
        assert!(install(
            &mut t,
            &mut NoCoordinator,
            region,
            &mut [0u8; 256],
            bytes.len() as u32
        ));
    }

    fn frame(cmd: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = payload.to_vec();
        v.extend_from_slice(&crc32_frame(cmd, payload.len() as u32, payload).to_le_bytes());
        v
    }

    fn payload_of(tx: &[u8]) -> (u8, String) {
        let len = u32::from_le_bytes(tx[5..9].try_into().unwrap()) as usize;
        (tx[4], String::from_utf8_lossy(&tx[9..9 + len]).into_owned())
    }

    #[test]
    fn list_renders_rows_in_sector_order_then_system_rows_free_and_running() {
        let _g = crate::packages::test_support::lock();
        crate::packages::reset_for_test();
        let mut region = MemRegion::new(16, 8);
        crate::packages::rescan_region(&region);
        seed(&mut region, &papk("com.a", Some("App A")));
        seed(&mut region, &papk("com.b", None));
        let launcher: &'static [u8] =
            Box::leak(papk("picodroid.launcher", Some("Launcher")).into_boxed_slice());
        crate::packages::register_system(&[launcher]);
        crate::packages::set_running(Some("picodroid.launcher"));

        let mut pipe = VecPipe::new(frame(CMD_LIST, b""));
        handle_list(&mut pipe, &CoreDirectory, 0);
        let (status, text) = payload_of(&pipe.tx);
        assert_eq!(status, STATUS_OK);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5, "{text}");
        assert!(lines[0].starts_with("0\tcom.a\t7\t2.1\t"), "{}", lines[0]);
        assert!(lines[0].ends_with("\t-\tApp A"), "{}", lines[0]);
        assert!(lines[1].starts_with("2\tcom.b\t7\t2.1\t"), "{}", lines[1]);
        assert!(lines[1].ends_with("\t-\tcom.b"), "{}", lines[1]);
        assert!(
            lines[2].starts_with("system\tpicodroid.launcher\t7\t2.1\t"),
            "{}",
            lines[2]
        );
        assert!(lines[2].ends_with("\t-\tLauncher"), "{}", lines[2]);
        // 16 sectors, two 2-sector runs: 12 free, all contiguous; the
        // system app takes no sector and does not count as installed.
        assert_eq!(
            lines[3],
            format!(
                "free\t{}\t{}\t2\t{}",
                12 * SECTOR,
                12 * SECTOR,
                crate::packages::MAX_INSTALLED_APPS
            )
        );
        assert_eq!(lines[4], "running\tpicodroid.launcher");
    }

    #[test]
    fn uninstall_of_an_unknown_package_is_not_found() {
        let _g = crate::packages::test_support::lock();
        crate::packages::reset_for_test();
        let region = MemRegion::new(8, 8);
        crate::packages::rescan_region(&region);
        let mut region = region;
        let name = b"com.nope";
        let mut pipe = VecPipe::new(frame(CMD_UNINSTALL, name));
        let reset = handle_uninstall(
            &mut pipe,
            &mut NoCoordinator,
            &mut region,
            &mut CoreDirectory,
            name.len() as u32,
        );
        assert!(!reset);
        let (status, text) = payload_of(&pipe.tx);
        assert_eq!(status, STATUS_NOT_FOUND);
        assert_eq!(text, "not installed");
        assert!(region.ops.is_empty());
    }

    /// The real loop ends in a reset; here the handler reports that it is
    /// due, after the run was erased and success reported.
    #[test]
    fn uninstall_erases_the_run_reports_ok_then_asks_for_a_reset() {
        let _g = crate::packages::test_support::lock();
        crate::packages::reset_for_test();
        let mut region = MemRegion::new(8, 8);
        crate::packages::rescan_region(&region);
        seed(&mut region, &papk("com.a", None));
        assert_eq!(crate::packages::installed_count(), 1);
        region.ops.clear();

        let name = b"com.a";
        let mut pipe = VecPipe::new(frame(CMD_UNINSTALL, name));
        let reset = handle_uninstall(
            &mut pipe,
            &mut NoCoordinator,
            &mut region,
            &mut CoreDirectory,
            name.len() as u32,
        );
        assert!(reset, "no reset after uninstall");
        assert_eq!(pipe.tx[4], STATUS_OK);
        assert_eq!(
            region.ops,
            vec![crate::install::mem_region::Op::Erase(0, 2)]
        );
        assert_eq!(crate::packages::installed_count(), 0);
    }
}
