// SPDX-License-Identifier: GPL-3.0-only
//! The simulator's half of the debug bridge — what `platforms/rp/src/pdb/`
//! is to a device.
//!
//! The protocol is [`crate::pdb`], unchanged: the same command loop, framing,
//! install orchestration, sysmon encoding and input injection a device runs.
//! What a family has to supply, this module supplies for the host:
//!
//! - [`SimTransport`], the byte pipe — a Unix-domain socket where a device
//!   has USB CDC. `tools/pdb` opens it with `-s <socket>` or `-s sim`, and
//!   `pdb devices` lists every one it finds.
//! - [`SimCoordinator`], the park handshake — the JVM task stops the app,
//!   drains its children and blocks while the bridge writes the region, the
//!   same choreography as `platforms/rp/src/pdb/coordinator.rs` and the
//!   supervisor loop in `boot_tasks.rs`, spelled through the `rtos` seam.
//! - [`SimSysmon`], task statistics from the hosted kernel.
//! - [`SimPapkFlash`], a handle onto the app region in `super::app_region`,
//!   whose reset *replaces the process*: the region is dumped to a file, the
//!   binary is exec'd again, and the new process boots warm from the dump —
//!   the real boot path, rescan and boot policy included, because the POSIX
//!   port cannot restart its scheduler in place (`rtos_freertos.rs`).
//!
//! # Several simulators at once
//!
//! Every simulator listens on its own socket, `pdb-<pid>.sock` under
//! [`socket_dir`] unless `PICODROID_SIM_PDB_SOCKET` names one, so any number
//! can run side by side (parallel sessions, a nightly lane beside a
//! developer's window). `pdb devices` probes them all; `pdb -s sim` picks the
//! lone one and refuses, listing the candidates, when there are several —
//! name the socket then. The process id survives the exec reboot, so the
//! default path is stable across it.
//!
//! # Blocking through the kernel
//!
//! This task outranks the JVM. On the POSIX port a task blocked in a host
//! read looks *running* to the kernel, and a running high-priority task
//! starves everything below it (`rtos_freertos.rs`, "The one invariant").
//! So the socket is non-blocking and every wait here is a `vTaskDelay`.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use crate::install::{CoreCoordinator, PapkFlash};
use crate::pdb::{PdbTransport, SysmonSample, SysmonSource, TaskSample, MAX_TASKS};
use crate::rtos::{self, Timeout};

use super::app_region;

/// Names the socket the bridge listens on; the default is
/// `<socket_dir>/pdb-<pid>.sock`.
pub const SOCKET_ENV: &str = "PICODROID_SIM_PDB_SOCKET";

/// Set on the process an exec reboot starts: the region dump it boots from
/// (`super::app_region::init`).
pub const WARM_BOOT_ENV: &str = "PICODROID_SIM_WARM_BOOT";

/// The install stream's per-byte timeout, as the device's USB transport
/// keeps it (`hal/rp/pdb_usb.rs`: 2 s).
const STREAM_TIMEOUT_MS: u32 = 2_000;

/// How long the bridge waits for the JVM task to park, in one-second
/// rounds — the device's fifteen (`coordinator.rs`).
const PARK_WAIT_ROUNDS: u32 = 15;

/// Bytes read from the socket per poll.
const RX_BUF: usize = 4096;

/// The directory every simulator's socket lives in: `picodroid-sim` under
/// the platform's temp dir. `tools/pdb` computes the same path to find them.
pub fn socket_dir() -> PathBuf {
    std::env::temp_dir().join("picodroid-sim")
}

/// This simulator's socket path.
pub fn socket_path() -> PathBuf {
    match std::env::var_os(SOCKET_ENV).filter(|s| !s.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => socket_dir().join(format!("pdb-{}.sock", std::process::id())),
    }
}

// ── Transport ────────────────────────────────────────────────────────────────

/// PDBP over a Unix-domain socket.
///
/// One client at a time, as one host holds a device's CDC port: a second
/// connection waits in the listen backlog until the first hangs up. A client
/// that leaves mid-frame is followed by the next one's bytes and the loop
/// resyncs on the frame magic, as the device's does.
pub struct SimTransport {
    listener: Option<UnixListener>,
    client: Option<UnixStream>,
    buf: [u8; RX_BUF],
    head: usize,
    len: usize,
    stream_timeout_ms: u32,
}

impl Default for SimTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl SimTransport {
    pub fn new() -> Self {
        Self {
            listener: None,
            client: None,
            buf: [0; RX_BUF],
            head: 0,
            len: 0,
            stream_timeout_ms: STREAM_TIMEOUT_MS,
        }
    }

    /// A transport already connected to `client`, for tests: no listener,
    /// and a stream timeout of `stream_timeout_ms`.
    #[cfg(test)]
    fn connected(client: UnixStream, stream_timeout_ms: u32) -> Self {
        client.set_nonblocking(true).expect("nonblocking");
        Self {
            client: Some(client),
            stream_timeout_ms,
            ..Self::new()
        }
    }

    /// One buffered byte, or one poll of the socket: a pending connection
    /// when no client is attached, a read when one is. `None` when nothing
    /// is there right now — never blocks.
    fn poll_once(&mut self) -> Option<u8> {
        if self.len > 0 {
            let b = self.buf[self.head];
            self.head += 1;
            self.len -= 1;
            return Some(b);
        }
        if self.client.is_none() {
            let listener = self.listener.as_ref()?;
            match listener.accept() {
                Ok((stream, _)) => {
                    if stream.set_nonblocking(true).is_err() {
                        return None;
                    }
                    self.client = Some(stream);
                }
                Err(_) => return None, // WouldBlock, Interrupted: nothing pending
            }
        }
        let client = self.client.as_mut()?;
        match client.read(&mut self.buf) {
            Ok(0) => {
                // The host hung up; the next poll accepts the next one.
                self.client = None;
                None
            }
            Ok(n) => {
                self.head = 1;
                self.len = n - 1;
                Some(self.buf[0])
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => None,
            Err(_) => {
                self.client = None;
                None
            }
        }
    }

    /// How long to sleep between empty polls: a millisecond with a host
    /// attached, ten while waiting for one — an idle simulator then costs
    /// a hundred kernel wakeups a second rather than a thousand.
    fn idle_ms(&self) -> u32 {
        if self.client.is_some() {
            1
        } else {
            10
        }
    }
}

impl PdbTransport for SimTransport {
    fn init(&mut self) {
        // Host-side objects: the listener, its path, the log line. None of
        // it has a device analog, so none of it is charged to the arena.
        let _host = crate::host::heap_bypass();
        let path = socket_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // A socket file a killed simulator left behind (the same pid, or
        // an explicit PICODROID_SIM_PDB_SOCKET) would refuse the bind.
        let _ = std::fs::remove_file(&path);
        match UnixListener::bind(&path) {
            Ok(listener) => {
                if let Err(e) = listener.set_nonblocking(true) {
                    eprintln!(
                        "[sim] pdb: cannot make {} non-blocking: {e}",
                        path.display()
                    );
                    return;
                }
                println!("[sim] pdb: listening on {}", path.display());
                self.listener = Some(listener);
            }
            Err(e) => eprintln!("[sim] pdb: cannot listen on {}: {e}", path.display()),
        }
        register_pdb_task();
    }

    fn read_byte(&mut self) -> u8 {
        loop {
            if let Some(b) = self.poll_once() {
                return b;
            }
            rtos::delay_ms(self.idle_ms());
        }
    }

    fn read_byte_timeout(&mut self) -> Option<u8> {
        // Only reached while streaming an install, so a host is attached and
        // every empty poll is one millisecond: the bound is the timeout.
        for _ in 0..self.stream_timeout_ms {
            if let Some(b) = self.poll_once() {
                return Some(b);
            }
            rtos::delay_ms(1);
        }
        None
    }

    fn write_bytes(&mut self, data: &[u8]) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let mut written = 0;
        // Responses are at most a couple of kilobytes, far under the socket
        // buffer, so this loop runs once; the retry is for a host that is
        // slow to read, and the cap keeps a stuck one from holding the task.
        for _ in 0..self.stream_timeout_ms {
            match client.write(&data[written..]) {
                Ok(n) => {
                    written += n;
                    if written == data.len() {
                        return;
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {
                    rtos::delay_ms(1);
                }
                // EPIPE and friends: the host is gone (Rust ignores SIGPIPE,
                // so this is an error, not a signal). Drop it and listen on.
                Err(_) => break,
            }
        }
        self.client = None;
    }

    fn drain_tx(&mut self) {
        // Nothing buffered on our side: `write_bytes` hands every byte to
        // the kernel, which delivers them to the peer even after the exec
        // reboot closes this end.
        if let Some(client) = self.client.as_mut() {
            let _ = client.flush();
        }
    }
}

// ── Park coordinator ─────────────────────────────────────────────────────────

/// Set by the bridge before it writes the region; the JVM task parks when it
/// sees this after the app has stopped (`platforms/rp/src/pdb/pending.rs`,
/// `FLASH_PARK_REQUESTED`).
static PARK_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Set by the JVM task once it has parked; cleared by the bridge to let it
/// go (`CORE0_PARKED` on a device).
static PARKED: AtomicBool = AtomicBool::new(false);

/// The JVM task's handle, for the bridge to wake it. Zero until it runs.
static JVM_TASK: AtomicUsize = AtomicUsize::new(0);

/// The bridge task's handle, for the JVM task to wake it once parked.
static PDB_TASK: AtomicUsize = AtomicUsize::new(0);

/// Record the calling task as the JVM task. The first thing its body does.
pub fn register_jvm_task() {
    JVM_TASK.store(rtos::task_current(), Ordering::Release);
}

/// Record the calling task as the bridge task.
fn register_pdb_task() {
    PDB_TASK.store(rtos::task_current(), Ordering::Release);
}

/// Wake the JVM task wherever it waits: its task notification, and the main
/// queue the activity loop blocks on (`pending::notify_jvm` on a device).
fn notify_jvm() {
    rtos::task_notify(JVM_TASK.load(Ordering::Acquire));
    // Not in this crate's test build: the main queue is a process-wide
    // static that `main_queue`'s own tests expect to find empty, and the
    // thread standing in for the JVM task in the tests below waits on its
    // task notification alone.
    #[cfg(not(test))]
    let _ = crate::executors::main_queue::enqueue_wake();
}

/// Has the bridge asked the JVM task to park? Polled by the app-switching
/// loop once the app has stopped and its children are gone.
pub fn park_requested() -> bool {
    PARK_REQUESTED.load(Ordering::Acquire)
}

/// Park the calling (JVM) task until the bridge releases it — the device's
/// park point (`boot_tasks.rs`): signal, wake the bridge, block. Returns
/// only on the error path, when an install was refused; a completed one
/// reboots the process instead.
pub fn park_until_released() {
    PARKED.store(true, Ordering::Release);
    rtos::task_notify(PDB_TASK.load(Ordering::Acquire));
    // "Look again", not a credit: the wait collects notifications it never
    // asked for, so the flag is what ends it.
    while PARKED.load(Ordering::Acquire) {
        rtos::task_wait_notification(Timeout::Forever);
    }
}

/// Stops the app and parks the JVM task so the bridge can write the region.
pub struct SimCoordinator;

impl CoreCoordinator for SimCoordinator {
    fn request_stop_and_park(&mut self) {
        PARKED.store(false, Ordering::Release);
        super::platform::set_stop_jvm(true);
        PARK_REQUESTED.store(true, Ordering::Release);
        notify_jvm();
    }

    fn wait_for_park(&mut self) -> bool {
        // The device's loop: fifteen one-second waits, the flag re-checked
        // on every wake because a notification is "look again".
        for _ in 0..PARK_WAIT_ROUNDS {
            if PARKED.load(Ordering::Acquire) {
                return true;
            }
            rtos::task_wait_notification(Timeout::Ms(1000));
        }
        PARKED.load(Ordering::Acquire)
    }

    fn release(&mut self) {
        PARKED.store(false, Ordering::Release);
        notify_jvm();
    }

    fn cancel_park_request(&mut self) {
        PARK_REQUESTED.store(false, Ordering::Release);
        // The installer calls `release` and then this. A JVM task that
        // reaches its park point between the two sees the request still up,
        // parks, and would wait for an install that is not coming — so the
        // cancel releases too. (The device's coordinator has the same window;
        // docs/designs/sim-pdb-endpoint-2026-09.md notes it.)
        PARKED.store(false, Ordering::Release);
        notify_jvm();
    }
}

// ── Task statistics ──────────────────────────────────────────────────────────

/// `TaskStatus_t` as the hosted kernel lays it out — pinned to
/// `crates/picodroid-core/freertos-host/FreeRTOSConfig.h` the way the
/// device's mirror is pinned to its own config:
/// `configUSE_TRACE_FACILITY 1`, `configNUMBER_OF_CORES 1` (so no affinity
/// field), `configSTACK_DEPTH_TYPE uint32_t`, `configRUN_TIME_COUNTER_TYPE`
/// defaulted to `uint32_t`, `configRECORD_STACK_HIGH_ADDRESS` undefined, on
/// a 64-bit host where `UBaseType_t` is `unsigned long`. 72 bytes, 8-aligned;
/// the test below pins that.
///
/// `freertos_rust`'s `FreeRtosUBaseType` is `u32`, which is why the FFI
/// below is declared here with `c_ulong` rather than through that crate.
#[repr(C)]
struct TaskStatusHost {
    handle: *const libc::c_void,
    task_name: *const libc::c_char,
    task_number: libc::c_ulong,
    task_state: libc::c_int, // eTaskState, a C enum
    current_priority: libc::c_ulong,
    base_priority: libc::c_ulong,
    run_time_counter: u32,
    stack_base: *const libc::c_void,
    stack_high_water_mark: u32,
}

extern "C" {
    fn uxTaskGetSystemState(
        pxTaskStatusArray: *mut TaskStatusHost,
        uxArraySize: libc::c_ulong,
        pulTotalRunTime: *mut u32,
    ) -> libc::c_ulong;
    fn uxTaskGetNumberOfTasks() -> libc::c_ulong;
}

/// Task and heap statistics from the hosted kernel and the modeled arena.
///
/// Run time is not generated on the host (`configGENERATE_RUN_TIME_STATS
/// 0`), so CPU share reads as unavailable; everything else is the kernel's.
pub struct SimSysmon;

impl SysmonSource for SimSysmon {
    fn sample(&mut self, out: &mut SysmonSample) -> bool {
        let mut buf: [core::mem::MaybeUninit<TaskStatusHost>; MAX_TASKS] =
            [const { core::mem::MaybeUninit::uninit() }; MAX_TASKS];
        let mut total_run_time: u32 = 0;

        // SAFETY: a plain kernel accessor.
        let n = unsafe { uxTaskGetNumberOfTasks() } as usize;
        if n > MAX_TASKS {
            // The kernel fills nothing when the array is short (the device
            // source warns the same way).
            eprintln!(
                "[sim] pdb sysmon: {n} tasks exceed MAX_TASKS={MAX_TASKS}; table will be empty"
            );
        }
        // SAFETY: `buf` is MAX_TASKS entries of the layout above, and the
        // count passed is clamped to that.
        let count = unsafe {
            uxTaskGetSystemState(
                buf.as_mut_ptr().cast(),
                n.min(MAX_TASKS) as libc::c_ulong,
                &mut total_run_time,
            )
        } as usize;
        let count = count.min(MAX_TASKS);

        // SAFETY: a plain kernel accessor.
        out.uptime_ticks = unsafe { freertos_rust::freertos_rs_xTaskGetTickCount() };
        // The shim reports the modeled device arena, as the device's
        // `xPortGetFreeHeapSize` reports its real one.
        out.free_heap = super::freertos_heap_shim::xPortGetFreeHeapSize() as u32;
        out.min_free_heap = super::freertos_heap_shim::xPortGetMinimumEverFreeHeapSize() as u32;
        out.total_run_time = total_run_time;
        out.task_count = count as u8;

        for (slot, entry) in out.tasks.iter_mut().zip(buf.iter()).take(count) {
            // SAFETY: the first `count` entries were filled above.
            let t = unsafe { entry.assume_init_ref() };
            let mut name = [0u8; 16];
            for (i, dst) in name.iter_mut().enumerate() {
                // SAFETY: FreeRTOS guarantees a NUL-terminated name.
                let b = unsafe { *t.task_name.add(i) } as u8;
                if b == 0 {
                    break;
                }
                *dst = b;
            }
            *slot = TaskSample {
                name,
                state: t.task_state as u8,
                current_priority: t.current_priority as u8,
                base_priority: t.base_priority as u8,
                stack_high_water: t.stack_high_water_mark as u16,
                task_number: t.task_number as u16,
                run_time: t.run_time_counter,
            };
        }
        true
    }
}

// ── The region, and the reboot ───────────────────────────────────────────────

/// Set by [`SimPapkFlash::trigger_reset`]; read by the main thread once the
/// scheduler has ended.
static REBOOT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The app region as the bridge's [`PapkFlash`]: a handle onto
/// `super::app_region`'s region, written only while the JVM task is parked
/// (the installer's contract, kept here exactly as on a device).
pub struct SimPapkFlash;

// SAFETY: every write forwards to `MemRegion`, which bounds-checks; the
// region is leaked for the process, so `mapped_base` stays valid; and the
// bridge writes only between a granted park and its release or the reboot.
unsafe impl PapkFlash for SimPapkFlash {
    fn region_len(&self) -> usize {
        app_region::with_region(|r| r.region_len()).unwrap_or(0)
    }

    fn max_installed_apps(&self) -> usize {
        app_region::with_region(|r| r.max_installed_apps()).unwrap_or(0)
    }

    fn mapped_base(&self) -> *const u8 {
        app_region::with_region(|r| r.mapped_base()).unwrap_or(core::ptr::null())
    }

    fn select_run(&mut self, first_sector: u32) {
        app_region::with_region(|r| r.select_run(first_sector));
    }

    unsafe fn erase_run(&mut self, first_sector: u32, sectors: u32) {
        app_region::with_region(|r| r.erase_run(first_sector, sectors));
    }

    unsafe fn write_page(&mut self, page_index: u32, page: &[u8; 256]) -> bool {
        app_region::with_region(|r| r.write_page(page_index, page)).unwrap_or(false)
    }

    unsafe fn write_meta_header(&mut self, len: u32, flags: u32, seq: u32) {
        app_region::with_region(|r| r.write_meta_header(len, flags, seq));
    }

    unsafe fn write_meta_commit(&mut self) {
        app_region::with_region(|r| r.write_meta_commit());
    }

    unsafe fn commit_metadata(&mut self, len: u32, flags: u32, seq: u32) {
        app_region::with_region(|r| r.commit_metadata(len, flags, seq));
    }

    unsafe fn copy_page(&mut self, src_sector: u32, dst_sector: u32, page: u32) {
        app_region::with_region(|r| r.copy_page(src_sector, dst_sector, page));
    }

    /// A device resets the chip. Here the scheduler is ended — which joins
    /// the tick thread and releases the main thread from `start_scheduler`
    /// — and the main thread, with no task left running, dumps the region
    /// and execs the binary ([`reboot`]). This task parks in the kernel's
    /// end-scheduler wait and is gone with the exec.
    fn trigger_reset(&mut self) -> ! {
        REBOOT_REQUESTED.store(true, Ordering::Release);
        println!("[sim] reboot: requested by the debug bridge; ending the scheduler");
        let _ = std::io::stdout().flush();
        #[cfg(not(test))]
        super::rtos::end_scheduler();
        unreachable!("the scheduler ended but this task ran on")
    }
}

/// Did the bridge ask for a reboot? Cleared by the call; `sim_boot::main`
/// asks once the scheduler has returned.
pub fn take_reboot_request() -> bool {
    REBOOT_REQUESTED.swap(false, Ordering::AcqRel)
}

/// Remove this simulator's socket file: the process is exiting for good
/// (nothing left to run), so `pdb devices` need not find and prune it. A
/// killed simulator skips this, and the scan prunes its socket instead.
pub fn shutdown() {
    let _ = std::fs::remove_file(socket_path());
}

/// Reboot the simulator: dump the region, then replace this process with
/// the same binary, arguments and environment, marked as a warm boot. Called
/// on the main thread after the scheduler has ended, so no task is mid-write.
///
/// Everything std opened is close-on-exec — the socket, the control FIFO,
/// the filesystem image, the window's X connection — and is reopened by
/// the new process from the inherited environment; stdout and stderr carry
/// on, so a log keeps flowing to the same file. The socket path is passed
/// explicitly so it stays put even where the temp dir would not.
pub fn reboot() -> ! {
    use std::os::unix::process::CommandExt;

    let snapshot = socket_dir().join(format!("apps-{}.img", std::process::id()));
    if let Err(e) = app_region::snapshot_to(&snapshot) {
        eprintln!("[sim] reboot: cannot write {}: {e}", snapshot.display());
        std::process::exit(1);
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            eprintln!("[sim] reboot: cannot find this binary: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "[sim] reboot: exec {} (warm boot from {})",
        exe.display(),
        snapshot.display()
    );
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    let err = std::process::Command::new(&exe)
        .args(std::env::args_os().skip(1))
        .env(WARM_BOOT_ENV, &snapshot)
        .env(SOCKET_ENV, socket_path())
        .exec();
    eprintln!("[sim] reboot: exec failed: {err}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The coordinator's statics are process-wide; its tests take turns.
    static HANDSHAKE: Mutex<()> = Mutex::new(());

    #[test]
    fn the_host_task_status_mirror_is_72_bytes_8_aligned() {
        assert_eq!(core::mem::size_of::<TaskStatusHost>(), 72);
        assert_eq!(core::mem::align_of::<TaskStatusHost>(), 8);
    }

    #[test]
    fn bytes_written_by_the_host_come_out_one_at_a_time() {
        let (ours, mut theirs) = UnixStream::pair().unwrap();
        let mut t = SimTransport::connected(ours, 50);
        theirs.write_all(b"PDBP\x00").unwrap();
        assert_eq!(t.read_byte(), b'P');
        assert_eq!(t.read_byte(), b'D');
        assert_eq!(t.read_byte(), b'B');
        assert_eq!(t.read_byte(), b'P');
        assert_eq!(t.read_byte_timeout(), Some(0));
        // Nothing more: the stream timeout, not a hang.
        assert_eq!(t.read_byte_timeout(), None);
    }

    #[test]
    fn a_response_reaches_the_host() {
        let (ours, mut theirs) = UnixStream::pair().unwrap();
        let mut t = SimTransport::connected(ours, 50);
        t.write_bytes(b"PDBP\x00\x00\x00\x00\x00");
        t.drain_tx();
        let mut got = [0u8; 9];
        theirs.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"PDBP\x00\x00\x00\x00\x00");
    }

    #[test]
    fn a_host_that_hangs_up_is_dropped_and_the_next_one_accepted() {
        let dir = std::env::temp_dir().join(format!("picodroid-pdb-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.sock");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();

        let (ours, theirs) = UnixStream::pair().unwrap();
        let mut t = SimTransport::connected(ours, 50);
        t.listener = Some(listener);
        drop(theirs); // the first host hangs up
        assert_eq!(t.read_byte_timeout(), None);
        assert!(t.client.is_none(), "a hung-up host is dropped");

        let mut second = UnixStream::connect(&path).unwrap();
        second.write_all(b"Z").unwrap();
        assert_eq!(t.read_byte(), b'Z');
        assert!(t.client.is_some(), "the next host is accepted");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// A thread standing in for the JVM task: waits for the park request,
    /// parks, and reports when it was released.
    fn jvm_stand_in() -> std::thread::JoinHandle<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        let h = std::thread::spawn(move || {
            register_jvm_task();
            tx.send(()).unwrap();
            while !park_requested() {
                rtos::task_wait_notification(Timeout::Ms(20));
            }
            park_until_released();
        });
        rx.recv().unwrap();
        h
    }

    #[test]
    fn the_bridge_parks_the_jvm_and_releases_it() {
        let _turn = HANDSHAKE.lock().unwrap_or_else(|p| p.into_inner());
        register_pdb_task();
        let jvm = jvm_stand_in();
        let mut c = SimCoordinator;
        c.request_stop_and_park();
        assert!(c.wait_for_park(), "the JVM task parked");
        assert!(
            super::super::platform::stop_jvm(),
            "the app was told to stop"
        );
        c.release();
        c.cancel_park_request();
        jvm.join().expect("released");
        super::super::platform::set_stop_jvm(false);
    }

    #[test]
    fn a_park_that_lands_after_the_release_is_still_let_go() {
        let _turn = HANDSHAKE.lock().unwrap_or_else(|p| p.into_inner());
        register_pdb_task();
        let mut c = SimCoordinator;
        // The bridge gave up (park timeout) and released before the JVM
        // task ever saw the request…
        c.request_stop_and_park();
        c.release();
        // …then the JVM task arrives, sees the request still up, and parks.
        let jvm = jvm_stand_in();
        while !PARKED.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // The cancel is what lets it go.
        c.cancel_park_request();
        jvm.join().expect("released by the cancel");
        super::super::platform::set_stop_jvm(false);
    }
}
