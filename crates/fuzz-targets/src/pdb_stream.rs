// SPDX-License-Identifier: GPL-3.0-only
//! The device's debug-bridge loop over whatever is on the other end of the
//! cable: a byte stream that may be garbage, truncated frames, hostile
//! lengths, a PAPK, or all of them in a row.

use std::cell::RefCell;
use std::sync::OnceLock;

use class_link::build::simple_class;
use papk_format::{EntryPoint, ManifestSpec, PapkBuilder};
use pd_install::mem_region::{MemRegion, MemTransport, NoCoordinator};
use pd_install::PapkFlash;
use pd_pdb::vec_pipe::VecPipe;
use pd_pdb::{serve_one, BridgeState, Served, SysmonSample, SysmonSource, RESYNC_LIMIT};
use pdb_protocol::greeting::Greeting;
use pdb_protocol::sysmon::SysmonView;
use pdb_protocol::{
    crc32_frame, CMD_INSTALL, CMD_LIST, CMD_PING, CMD_SYSMON, FRAME_MAGIC, STATUS_CRC_FAIL,
    STATUS_ERR, STATUS_INCOMPAT, STATUS_NOT_FOUND, STATUS_NO_ROOM, STATUS_OK, STATUS_READY,
    STATUS_TOO_LARGE,
};

use crate::stubs::{FirstFit, FRAMEWORK_MAP_VERSION};

/// 16 sectors: room for a few seed apps.
const SECTORS: usize = 16;

thread_local! {
    // One region for the process: `MemRegion::new` leaks its buffer.
    static REGION: RefCell<MemRegion> = RefCell::new(MemRegion::new(SECTORS, 8));
}

/// A small valid PAPK for `com.a`, built once.
fn seed_papk() -> &'static [u8] {
    static PAPK: OnceLock<Vec<u8>> = OnceLock::new();
    PAPK.get_or_init(|| {
        let main = simple_class(b"a/Main", b"java/lang/Object", Some(&[0x03, 0xAC]));
        let mut b = PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("a/Main"),
            package_name: "com.a",
            version: "1.0",
            framework_map_version: FRAMEWORK_MAP_VERSION,
            version_code: Some(1),
            label: Some("A"),
            icon: None,
            design_size: None,
            requires_features: None,
        });
        b.class("a/Main", &main);
        b.build().unwrap()
    })
}

struct Sysmon(bool);

impl SysmonSource for Sysmon {
    fn sample(&mut self, out: &mut SysmonSample) -> bool {
        *out = SysmonSample {
            uptime_ticks: 1000,
            free_heap: 4096,
            task_count: 1,
            total_run_time: 2000,
            ..Default::default()
        };
        out.tasks[0].name[..3].copy_from_slice(b"jvm");
        out.tasks[0].run_time = 500;
        self.0
    }
}

/// One response frame off the wire.
struct Reply<'a> {
    status: u8,
    payload: &'a [u8],
}

/// Split `tx` into `[PDBP][status][len][payload]` frames; a trailing partial
/// frame is a finding.
fn replies(tx: &[u8]) -> Vec<Reply<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < tx.len() {
        assert!(
            tx.len() - at >= 9 && &tx[at..at + 4] == FRAME_MAGIC,
            "a partial or unframed reply at {at}: {:?}",
            &tx[at..]
        );
        let status = tx[at + 4];
        let len = u32::from_le_bytes(tx[at + 5..at + 9].try_into().unwrap()) as usize;
        assert!(tx.len() - at - 9 >= len, "a reply shorter than its length");
        out.push(Reply {
            status,
            payload: &tx[at + 9..at + 9 + len],
        });
        at += 9 + len;
    }
    out
}

const KNOWN_STATUS: [u8; 8] = [
    STATUS_OK,
    STATUS_READY,
    STATUS_ERR,
    STATUS_TOO_LARGE,
    STATUS_CRC_FAIL,
    STATUS_INCOMPAT,
    STATUS_NO_ROOM,
    STATUS_NOT_FOUND,
];

fn check_reply(cmd: u8, r: &Reply<'_>, multi: bool) {
    assert!(
        KNOWN_STATUS.contains(&r.status),
        "cmd {cmd}: status {:#x} the host does not know",
        r.status
    );
    if r.status != STATUS_OK {
        return;
    }
    match cmd {
        CMD_PING => {
            let g = Greeting::parse(r.payload).expect("a greeting the host parses");
            assert_eq!(g.apps.is_some(), multi, "apps field follows MULTI_APP");
            assert_eq!(g.framework_map_version, FRAMEWORK_MAP_VERSION);
        }
        CMD_SYSMON => {
            if !r.payload.is_empty() {
                SysmonView::parse(r.payload).expect("a sysmon table the host parses");
            }
        }
        CMD_LIST => {
            core::str::from_utf8(r.payload).expect("a list in UTF-8");
        }
        _ => {}
    }
}

fn run<const MULTI: bool>(rx: &[u8], sysmon_available: bool, preinstall: bool) {
    REGION.with_borrow_mut(|region| {
        region.erase_all();
        let mut device = FirstFit::<MULTI>::new(SECTORS as u32);
        let mut state = BridgeState::new();
        let mut sysmon = Sysmon(sysmon_available);
        if preinstall {
            let papk = seed_papk();
            let mut t = MemTransport::for_papk(papk);
            assert!(pd_install::install(
                &mut t,
                &mut NoCoordinator,
                region,
                &mut device,
                &mut state.page,
                FRAMEWORK_MAP_VERSION,
                papk.len() as u32,
            ));
            assert_eq!(device.installed.len(), 1);
        }
        let max_payload = region.max_data_size().max(pdb_protocol::MAX_PACKAGE_NAME);
        let mut pipe = VecPipe::new(rx.to_vec());
        let mut frames = 0usize;
        while !pipe.exhausted() {
            frames += 1;
            assert!(frames <= rx.len() + 1, "more frames than bytes");
            pipe.reads = 0;
            let mark = pipe.tx.len();
            let served = serve_one(
                &mut pipe,
                &mut NoCoordinator,
                &mut sysmon,
                region,
                &mut device,
                &mut state,
            );
            // Resync, header, the largest payload any verb admits, CRC —
            // and nothing a hostile `len` names beyond that.
            assert!(
                pipe.reads <= RESYNC_LIMIT + 9 + max_payload + 4,
                "{} bytes read for one frame",
                pipe.reads
            );
            let rs = replies(&pipe.tx[mark..]);
            match served {
                Served::NoMagic => assert!(rs.is_empty(), "a reply with no frame"),
                Served::Answered { cmd, reset } => {
                    if cmd == CMD_INSTALL {
                        assert!(
                            (1..=2).contains(&rs.len()),
                            "{} replies to an install",
                            rs.len()
                        );
                        let last = rs.last().unwrap();
                        assert_eq!(
                            reset,
                            last.status == STATUS_OK,
                            "reset only after a committed install"
                        );
                        if reset {
                            assert_eq!(rs[0].status, STATUS_READY);
                            assert!(papk_format::flash_image::is_committed(
                                region.sector(device.installed.last().unwrap().first)
                            ));
                        }
                    } else {
                        assert_eq!(rs.len(), 1, "cmd {cmd}: {} replies", rs.len());
                        check_reply(cmd, &rs[0], MULTI);
                        if reset {
                            assert_eq!(cmd, pdb_protocol::CMD_UNINSTALL);
                            assert_eq!(rs[0].status, STATUS_OK);
                            let gone = device.removed.last().expect("a removed package");
                            assert!(device.installed.iter().all(|r| &r.package != gone));
                        } else {
                            assert!(
                                cmd != pdb_protocol::CMD_UNINSTALL || rs[0].status != STATUS_OK
                            );
                        }
                    }
                    if !MULTI && matches!(cmd, CMD_LIST | pdb_protocol::CMD_UNINSTALL) {
                        assert!(
                            matches!(rs[0].status, STATUS_ERR | STATUS_TOO_LARGE),
                            "a single-app device served {cmd}: {}",
                            rs[0].status
                        );
                    }
                }
            }
        }

        // Whatever came before, a well-formed PING is answered: the loop
        // carries nothing across frames.
        let mut ping = FRAME_MAGIC.to_vec();
        ping.push(CMD_PING);
        ping.extend_from_slice(&0u32.to_le_bytes());
        ping.extend_from_slice(&crc32_frame(CMD_PING, 0, &[]).to_le_bytes());
        pipe.rx = ping;
        pipe.pos = 0;
        pipe.tx.clear();
        let served = serve_one(
            &mut pipe,
            &mut NoCoordinator,
            &mut sysmon,
            region,
            &mut device,
            &mut state,
        );
        assert_eq!(
            served,
            Served::Answered {
                cmd: CMD_PING,
                reset: false
            },
            "the loop did not resync on a clean PING"
        );
        let rs = replies(&pipe.tx);
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].status, STATUS_OK);
        let g = Greeting::parse(rs[0].payload).unwrap();
        assert_eq!(g.apps.is_some(), MULTI);
        if let Some(apps) = g.apps {
            assert_eq!(apps.installed as usize, device.installed.len());
        }
    });
}

/// Input: a config byte (bit 0: statistics available; bit 1: a multi-app
/// firmware; bit 2: `com.a` installed first), then the bytes the host sends.
///
/// Every frame is answered exactly once with a status the host knows, or
/// skipped; no frame makes the loop read more than resync plus the largest
/// payload its verb admits; only a committed install or uninstall asks for a
/// reset; and a clean PING afterwards is answered whatever came before.
pub fn pdb_stream(data: &[u8]) {
    let (config, rx) = data.split_first().unwrap_or((&0, &[]));
    let sysmon_available = config & 1 != 0;
    let preinstall = config & 4 != 0;
    if config & 2 != 0 {
        run::<true>(rx, sysmon_available, preinstall);
    } else {
        run::<false>(rx, sysmon_available, preinstall);
    }
}
