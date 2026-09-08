// SPDX-License-Identifier: GPL-3.0-only
//! `CMD_LIST` and `CMD_UNINSTALL` — the package directory over the bridge
//! (docs/designs/multi-app-2026-09.md D8). Multi-app boards only; a
//! single-app board answers `unknown cmd`.
//!
//! The list is text, one tab-separated row per installed app, one per
//! system app, a `free` footer and a `running` line, so `tools/pdb` renders
//! it without a second wire format and a person can read the raw frame in
//! a serial capture. Rows only ever get added (multi-app M2 added the
//! system rows and the running line), so an older host still parses it.

use core::fmt::Write as _;

use pdb_protocol::{
    crc32_frame, CMD_LIST, CMD_UNINSTALL, MAX_PACKAGE_NAME, STATUS_CRC_FAIL, STATUS_ERR,
    STATUS_NOT_FOUND, STATUS_OK,
};

use super::framing::{send_response, Framed, TextBuf};
use super::PdbTransport;
use crate::install::{run_uninstall, CoreCoordinator, PapkFlash};
use crate::packages::{self, Kind, SECTOR};

/// Room for eight rows of a long package name and label, the system rows,
/// the footer and the running line.
const LIST_BUF: usize = 1280;

/// Handle `CMD_LIST`: rows `sector<TAB>package<TAB>version-code<TAB>version
/// <TAB>size<TAB>flags<TAB>label`, in sector order, then the system apps
/// with `system` in the sector column, then
/// `free<TAB>largest<TAB>total<TAB>installed<TAB>max` (bytes and counts),
/// then `running<TAB>package` when an app is running.
pub fn handle_list(transport: &mut impl PdbTransport, len: u32) {
    let wire_crc = transport.read_u32_le();
    if wire_crc != crc32_frame(CMD_LIST, len, &[]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }
    let mut text = TextBuf::<LIST_BUF>::new();
    render_list(&mut text);
    send_response(transport, STATUS_OK, text.as_bytes());
}

fn render_list(text: &mut TextBuf<LIST_BUF>) {
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

/// Handle `CMD_UNINSTALL`: payload is the package name. Parks the JVM,
/// erases the whole run, and resets — the same choreography as an install.
pub fn handle_uninstall(
    transport: &mut impl PdbTransport,
    coordinator: &mut impl CoreCoordinator,
    flash: &mut impl PapkFlash,
    len: u32,
) {
    // Drain the framed payload (bounded) + trailing CRC, keeping the byte
    // stream in sync even if the payload is malformed or oversized.
    let mut name = [0u8; MAX_PACKAGE_NAME];
    let n = (len as usize).min(MAX_PACKAGE_NAME);
    for b in name.iter_mut().take(n) {
        *b = transport.read_byte();
    }
    for _ in MAX_PACKAGE_NAME..len as usize {
        let _ = transport.read_byte();
    }
    let wire_crc = transport.read_u32_le();
    if wire_crc != crc32_frame(CMD_UNINSTALL, len, &name[..n]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }
    let Ok(package) = core::str::from_utf8(&name[..n]) else {
        send_response(transport, STATUS_ERR, b"package name is not UTF-8");
        return;
    };
    let (first, sectors) = match packages::find(package) {
        None => {
            send_response(transport, STATUS_NOT_FOUND, b"not installed");
            return;
        }
        Some(e) if e.kind == Kind::System => {
            send_response(transport, STATUS_ERR, b"system package");
            return;
        }
        Some(e) => (e.first_sector as u32, e.sectors as u32),
    };
    let mut framed = Framed(transport);
    run_uninstall(&mut framed, coordinator, flash, first, sectors);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::install;
    use crate::install::mem_region::{MemRegion, MemTransport, NoCoordinator};
    use crate::pdb::tests::MockPipe;
    use papk_format::{EntryPoint, ManifestSpec, PapkBuilder};

    fn papk(package: &str, label: Option<&str>) -> Vec<u8> {
        let mut b = PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("t/Main"),
            package_name: package,
            version: "2.1",
            framework_map_version: crate::framework_map::FRAMEWORK_MAP_VERSION,
            version_code: Some(7),
            label,
            icon: None,
        });
        b.class("t/Main", b"CAFE");
        b.build().unwrap()
    }

    fn seed(region: &mut MemRegion, bytes: &[u8]) {
        let mut t = MemTransport::for_papk(bytes);
        assert!(install(
            &mut t,
            &mut NoCoordinator,
            region,
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

        let mut pipe = MockPipe::new(frame(CMD_LIST, b""));
        handle_list(&mut pipe, 0);
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
    fn a_bad_crc_on_list_is_refused() {
        let _g = crate::packages::test_support::lock();
        crate::packages::reset_for_test();
        let mut pipe = MockPipe::new(vec![0, 0, 0, 0]);
        handle_list(&mut pipe, 0);
        assert_eq!(pipe.tx[4], STATUS_CRC_FAIL);
    }

    #[test]
    fn uninstall_of_an_unknown_package_is_not_found() {
        let _g = crate::packages::test_support::lock();
        crate::packages::reset_for_test();
        let region = MemRegion::new(8, 8);
        crate::packages::rescan_region(&region);
        let mut region = region;
        let name = b"com.nope";
        let mut pipe = MockPipe::new(frame(CMD_UNINSTALL, name));
        handle_uninstall(
            &mut pipe,
            &mut NoCoordinator,
            &mut region,
            name.len() as u32,
        );
        let (status, text) = payload_of(&pipe.tx);
        assert_eq!(status, STATUS_NOT_FOUND);
        assert_eq!(text, "not installed");
        assert!(region.ops.is_empty());
    }

    /// The real handler ends in a reset; the mock region panics instead of
    /// resetting, which is the signal that the run was erased and success
    /// reported first.
    #[test]
    fn uninstall_erases_the_run_reports_ok_then_resets() {
        let _g = crate::packages::test_support::lock();
        crate::packages::reset_for_test();
        let mut region = MemRegion::new(8, 8);
        crate::packages::rescan_region(&region);
        seed(&mut region, &papk("com.a", None));
        assert_eq!(crate::packages::installed_count(), 1);
        region.ops.clear();

        let name = b"com.a";
        let mut pipe = MockPipe::new(frame(CMD_UNINSTALL, name));
        let reset = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle_uninstall(
                &mut pipe,
                &mut NoCoordinator,
                &mut region,
                name.len() as u32,
            );
        }))
        .is_err();
        assert!(reset, "no reset after uninstall");
        assert_eq!(pipe.tx[4], STATUS_OK);
        assert_eq!(
            region.ops,
            vec![crate::install::mem_region::Op::Erase(0, 2)]
        );
        assert_eq!(crate::packages::installed_count(), 0);
    }
}
