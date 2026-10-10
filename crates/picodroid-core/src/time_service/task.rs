// SPDX-License-Identifier: GPL-3.0-only
//! The time task: anchors the wall clock from the network and keeps it there.
//!
//! One task per boot, spawned beside the other boot tasks on every board
//! with a network (`platforms/rp/src/boot_tasks.rs`, `sim_boot.rs`). It
//! sleeps until the link is up, then runs one SNTP exchange and anchors the
//! clock; after that it re-anchors every [`RESYNC_MS`], and after a failure
//! retries along [`RETRY_MS`]. Nothing here runs on the UI task: the DNS
//! lookup and the UDP receive block, and a tick is 16 ms.
//!
//! Event-driven, the shape of Android's `NetworkTimeUpdateService`: that
//! service listens for the connectivity broadcast and asks `AlarmManager`
//! for its next poll; this task sleeps on a kernel notification until the
//! next sync is due (hours), and is woken early by a link edge
//! ([`link_changed`]) or by a caller that needs the clock now
//! ([`request_sync_now`]: the TLS layer about to refuse a handshake, or
//! Settings turning automatic time back on). With the link down, or
//! automatic time off, it sleeps with no timeout at all. Nothing polls.
//!
//! Who delivers the link edge: on the device, the IP stack's event hook
//! (`hal::freertos_tcp::picodroid_net_ip_event`, on the IP task). The
//! simulator's link flips come from host threads outside the kernel, where
//! no kernel primitive may be touched, so there the edge reaches this task
//! the way it reaches `ConnectivityManager`: the UI loop sees
//! `LINK_CHANGES` move and kicks from the JVM task
//! (`lifecycle::net_events`). A link already up at boot needs no edge: the
//! first pass syncs at once.
//!
//! The simulator's host sockets reach the real `pool.ntp.org`, so a sim run
//! with a network board anchors itself the way a device does;
//! `PICODROID_SIM_NTP_SERVER=<host>` points it elsewhere and `=off` keeps the
//! task from syncing at all, for rows whose clock must be the host's
//! (`PICODROID_SIM_WALL_CLOCK=1`) or must stay unset.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::sntp;
use crate::hal::net;
use crate::rtos::{self, TaskKind, TaskSpec, Timeout};

/// The server. Every exchange resolves it afresh, so the pool rotates.
const SERVER: &str = "pool.ntp.org";
/// How long to wait for the reply.
const REPLY_TIMEOUT_MS: u32 = 3000;
/// Re-anchor cadence once the clock is set: six hours, as `picoenvmon` had.
const RESYNC_MS: u64 = 6 * 60 * 60 * 1000;
/// Retry ladder after a failed exchange, then the last entry forever.
const RETRY_MS: [u64; 4] = [5_000, 15_000, 60_000, 5 * 60 * 1000];
/// How long `wait_for_wall_clock` is prepared to wait, and how often it
/// looks: a lookup plus one exchange is well under two seconds on a joined
/// link, so a caller that waits this long had no network to speak of.
const WAIT_STEP_MS: u32 = 100;

/// The task's priority: the background tier below the interpreter, on both
/// platforms. The TLS handshake task takes the JVM tier in the simulator
/// because a task below it can starve behind a Java thread blocked in a
/// host socket call (`net/tls.rs`, `HANDSHAKE_PRIORITY`); this task must
/// not, for the inverse reason: its own DNS lookup and UDP receive are host
/// calls the POSIX port cannot see, and at the JVM's priority with time
/// slicing off a 3 s receive timeout would hold the core from the UI task
/// for 3 s. Below it, the tick hands the core back the moment the UI task
/// is ready, and a sync that waits for an idle gap is late, not wrong.
const PRIORITY: u8 = crate::task_priority::PRIORITY_BG_6;

/// The task's handle, for `request_sync_now`; 0 until the task runs.
static TASK: AtomicUsize = AtomicUsize::new(0);
/// A sync was asked for; cleared by the task when it acts on it.
static KICK: AtomicBool = AtomicBool::new(false);

/// Create the task. Pre-scheduler or after; either way it only acts once
/// the link reports up.
pub fn spawn() {
    let spec = TaskSpec {
        name: super::TASK_NAME,
        kind: TaskKind::BgWorker,
        priority: PRIORITY,
        stack_bytes: Some(super::TASK_STACK_BYTES),
    };
    if !rtos::spawn(&spec, alloc::boxed::Box::new(run)) {
        crate::pd_error!("time: task not created");
    }
}

/// Ask the task to sync as soon as the link allows. From a task context
/// only (a kernel notification); a no-op before the task runs.
pub fn request_sync_now() {
    KICK.store(true, Ordering::Release);
    link_changed();
}

/// The link went up or down: wake the task so it looks at the link now
/// (the connectivity event Android's time service subscribes to). From a
/// task context only; a no-op before the task runs. The task reads the
/// link state itself, so a stale or repeated wake costs one pass.
pub fn link_changed() {
    let t = TASK.load(Ordering::Acquire);
    if t != 0 {
        rtos::task_notify(t);
    }
}

/// Block the caller until the wall clock is anchored, or `max_ms` has
/// passed, or the link is down. True when the clock is set on return. For
/// a Java thread about to need the clock (a TLS handshake): the task is
/// asked for a sync at once, so the wait is one exchange long, not a retry
/// interval. Gives up the JVM run lock while it waits.
pub fn wait_for_wall_clock(max_ms: u32) -> bool {
    if super::clock_is_set() {
        return true;
    }
    if !net::is_network_up() || !super::auto_time() {
        return false;
    }
    request_sync_now();
    let mut waited = 0;
    while waited < max_ms {
        rtos::delay_ms(WAIT_STEP_MS);
        waited += WAIT_STEP_MS;
        if super::clock_is_set() {
            return true;
        }
    }
    false
}

fn now_ms() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

fn run() {
    TASK.store(rtos::task_current(), Ordering::Release);
    if !enabled() {
        crate::pd_info!("time: sync off (PICODROID_SIM_NTP_SERVER=off)");
        // Nothing to do, ever; park rather than return so the boot budget's
        // charge stays as the device model has it.
        loop {
            rtos::task_wait_notification(Timeout::Forever);
        }
    }
    // Due at once: a link already up at boot (the simulator's, or a device
    // whose join beat this task) is served on the first pass.
    let mut due_at: u64 = 0;
    let mut failures: usize = 0;
    let mut was_up = false;
    loop {
        let now = now_ms();
        let up = net::is_network_up();
        if up != was_up {
            was_up = up;
            if up {
                // A fresh link: sync now, with a clean retry ladder.
                due_at = now;
                failures = 0;
            }
        }
        // Load then store, not `swap`: thumbv6m has no atomic RMW, and a
        // kick that lands between the two only costs one extra pass.
        if KICK.load(Ordering::Acquire) {
            KICK.store(false, Ordering::Release);
            due_at = now;
        }
        if up && super::auto_time() && now >= due_at {
            match sync_once() {
                Ok(()) => {
                    failures = 0;
                    due_at = now_ms() + RESYNC_MS;
                }
                Err(()) => {
                    due_at = now_ms() + RETRY_MS[failures.min(RETRY_MS.len() - 1)];
                    failures += 1;
                }
            }
        }
        // Sleep until the next sync is due, or until a link edge or a
        // caller wakes it; with no link, or automatic time off, there is
        // nothing to wake for but those. A notification is a wake-up, not
        // a message: everything above is re-read on return.
        let wait = if up && super::auto_time() {
            let ms = due_at.saturating_sub(now_ms()).max(1);
            Timeout::Ms(ms.min(u32::MAX as u64) as u32)
        } else {
            Timeout::Forever
        };
        rtos::task_wait_notification(wait);
    }
}

/// One exchange: resolve, send, receive, anchor. Logs what happened either
/// way; `Err` means retry later.
fn sync_once() -> Result<(), ()> {
    let server = server();
    let addr = match net::dns_resolve(server) {
        Ok(a) => a,
        Err(e) => {
            crate::pd_warn!("time: {} did not resolve ({})", server, e.raw);
            return Err(());
        }
    };
    let sock = match net::udp_socket(0) {
        Ok(s) => s,
        Err(e) => {
            crate::pd_warn!("time: no UDP socket ({})", e.raw);
            return Err(());
        }
    };
    let result = exchange(sock, addr);
    net::close(sock);
    match result {
        Ok((epoch_ms, rtt_ms)) => {
            let before = crate::os::system_clock::wall_offset_ms();
            let elapsed_before = now_ms() as i64;
            crate::os::system_clock::anchor_wall_clock(epoch_ms);
            let step_ms = if before == 0 {
                0
            } else {
                epoch_ms - (elapsed_before + before)
            };
            super::note_sync(elapsed_before as u64);
            let o = addr.to_be_bytes();
            crate::pd_info!(
                "time: synced from {} ({}.{}.{}.{}), rtt {} ms, step {} ms",
                server,
                o[0],
                o[1],
                o[2],
                o[3],
                rtt_ms,
                step_ms
            );
            if super::sync_count() == 1 {
                if let Some(unused) = rtos::task_stack_unused_bytes() {
                    crate::pd_info!(
                        "time: task stack {} B, {} B unused",
                        super::TASK_STACK_BYTES,
                        unused
                    );
                }
            }
            Ok(())
        }
        Err(why) => {
            crate::pd_warn!("time: no usable reply from {}: {}", server, why);
            Err(())
        }
    }
}

/// Send one request to `addr` and wait for its reply: `(epoch ms at
/// arrival, round trip ms)`.
fn exchange(sock: *mut core::ffi::c_void, addr: u32) -> Result<(i64, i64), &'static str> {
    net::set_recv_timeout(sock, REPLY_TIMEOUT_MS);
    let nonce = crate::hal::system_clock::elapsed_realtime_nanos() as u64 | 1;
    let request = sntp::request(nonce);
    let sent_at = now_ms();
    net::udp_sendto(sock, &request, addr, sntp::PORT).map_err(|_| "send failed")?;
    let mut buf = [0u8; sntp::PACKET_LEN];
    // A stray datagram (another sender, a late reply to an earlier nonce)
    // is skipped; the socket's timeout bounds the whole wait.
    loop {
        let (len, from, _port) = net::udp_recvfrom(sock, &mut buf).map_err(|e| {
            if e.kind == crate::hal::types::NetErrorKind::TimedOut {
                "timed out"
            } else {
                "receive failed"
            }
        })?;
        if from != addr {
            continue;
        }
        let rtt_ms = now_ms().saturating_sub(sent_at) as i64;
        match sntp::parse_reply(&buf[..len], nonce, rtt_ms) {
            Ok(epoch_ms) => return Ok((epoch_ms, rtt_ms)),
            Err(sntp::ReplyError::Nonce) => continue,
            Err(e) => return Err(e.name()),
        }
    }
}

/// The server to ask. The simulator's `PICODROID_SIM_NTP_SERVER`
/// overrides the pool.
#[cfg(feature = "sim")]
fn server() -> &'static str {
    use std::sync::OnceLock;
    static SERVER_OVERRIDE: OnceLock<Option<String>> = OnceLock::new();
    SERVER_OVERRIDE
        .get_or_init(|| std::env::var("PICODROID_SIM_NTP_SERVER").ok())
        .as_deref()
        .unwrap_or(SERVER)
}

#[cfg(not(feature = "sim"))]
fn server() -> &'static str {
    SERVER
}

/// Whether the task syncs at all (`PICODROID_SIM_NTP_SERVER=off`).
#[cfg(feature = "sim")]
fn enabled() -> bool {
    !server().eq_ignore_ascii_case("off")
}

#[cfg(not(feature = "sim"))]
fn enabled() -> bool {
    true
}
