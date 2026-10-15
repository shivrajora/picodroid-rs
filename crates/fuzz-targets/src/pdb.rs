// SPDX-License-Identifier: GPL-3.0-only
//! PDBP payloads: the one the device decodes (`CMD_INPUT`) and the ones the
//! host decodes from whatever is on the other end of the cable (the PING
//! greeting, the sysmon table).

use pdb_protocol::greeting::{self, AppsInfo, Greeting, GREETING_MAX};
use pdb_protocol::input::{InputEvent, MAX_INPUT_PAYLOAD};
use pdb_protocol::sysmon::SysmonView;

use crate::Bytes;

/// Input: a selector byte, then a payload for the decoder it picks.
pub fn pdb_decode(data: &[u8]) {
    let mut b = Bytes::new(data);
    match b.u8() % 4 {
        0 => input(b.rest()),
        1 => {
            let payload = b.rest();
            if let Ok(g) = Greeting::parse(payload) {
                let _ = (g.minor(), g.is_legacy());
            }
        }
        2 => sysmon(b.rest()),
        _ => greeting_round_trip(&mut b),
    }
}

/// Whatever decodes re-encodes to bytes that decode to the same event.
fn input(payload: &[u8]) {
    let Ok(event) = InputEvent::decode(payload) else {
        return;
    };
    let mut out = [0u8; MAX_INPUT_PAYLOAD];
    let n = event.encode(&mut out);
    assert_eq!(
        InputEvent::decode(&out[..n]),
        Ok(event),
        "input event round trip"
    );
}

fn sysmon(payload: &[u8]) {
    let Ok(v) = SysmonView::parse(payload) else {
        return;
    };
    let _ = (v.uptime_ticks(), v.free_heap(), v.min_free_heap());
    let _ = (
        v.total_run_time(),
        v.expected_len(),
        v.payload_len(),
        v.mem_diag(),
    );
    // Past the declared count as well as inside it: `task` must refuse.
    for i in 0..v.task_count() + 2 {
        if let Some(t) = v.task(i) {
            let _ = (t.name(), t.state(), t.current_priority(), t.base_priority());
            let _ = (t.stack_high_water(), t.task_number(), t.cpu_permille());
        }
    }
}

/// The device's greeting encoder against the host's parser, over fields the
/// input chooses. A field within its cap and in UTF-8 comes back as sent.
fn greeting_round_trip(b: &mut Bytes<'_>) {
    let max_papk = b.u32();
    let fmv = b.take(greeting::FMV_MAX + 8).to_vec();
    let board = b.take(greeting::BOARD_MAX + 8).to_vec();
    let serial = b.take(greeting::SERIAL_MAX + 8).to_vec();
    let apps = (b.u8() & 1 == 1).then(|| AppsInfo {
        max: b.u8(),
        installed: b.u8(),
        largest_free: b.u32(),
        total_free: b.u32(),
    });
    let mut out = [0u8; GREETING_MAX];
    let n = match &apps {
        Some(a) => greeting::encode_with_apps(max_papk, &fmv, &board, &serial, a, &mut out),
        None => greeting::encode(max_papk, &fmv, &board, &serial, &mut out),
    };
    assert!(n <= GREETING_MAX);
    let fits = |f: &[u8], cap: usize| f.len() <= cap && core::str::from_utf8(f).is_ok();
    let parsed = Greeting::parse(&out[..n]);
    if fits(&fmv, greeting::FMV_MAX)
        && fits(&board, greeting::BOARD_MAX)
        && fits(&serial, greeting::SERIAL_MAX)
    {
        let g = parsed.expect("a greeting the device encodes, the host parses");
        assert_eq!(g.version, greeting::PROTOCOL_VERSION);
        assert_eq!(g.max_papk, max_papk);
        assert_eq!(g.framework_map_version.as_bytes(), &fmv[..]);
        assert_eq!(g.board.as_bytes(), &board[..]);
        assert_eq!(g.serial.as_bytes(), &serial[..]);
        assert_eq!(g.apps, apps);
    }
}
