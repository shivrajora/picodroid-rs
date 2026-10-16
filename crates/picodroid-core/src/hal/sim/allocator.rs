// SPDX-License-Identifier: GPL-3.0-only
//! Capped global allocator for the host simulator.
//!
//! Simulates the constrained FreeRTOS heap on RP2040/RP2350. **The cap is ON
//! by default**, sized to the simulated chip's `configTOTAL_HEAP_SIZE`
//! (docs/parity-audit.md MEM-03/M2), and capped mode is backed by a real
//! [`Heap4`] arena — a bit-faithful port of the device's heap_4 allocator
//! (MEM-01/M1) — so first-fit placement, header costs, and fragmentation
//! behave as they do on hardware. Override with `PICODROID_HEAP_LIMIT_KB`
//! (or `sim.sh -l`); the explicit value `0` disables the cap and falls back
//! to plain byte metering over the host allocator.
//!
//! Routing (mirrors the device, where the FreeRTOS heap serves *all*
//! firmware allocations):
//! - before [`arm`] (host runtime startup, argv/env) → host allocator,
//!   uncounted — those allocations have no device counterpart;
//! - inside a [`bypass`] region (host-only artifacts: minifb, the
//!   flash-modeled APK, spawned-thread internals) → host allocator,
//!   uncounted;
//! - otherwise → the heap_4 arena (or metered host alloc when uncapped).
//!
//! `dealloc` routes by pointer range (in-arena vs host), so alloc/free
//! pairings that straddle a bypass region are handled correctly by
//! construction — the old "balance rule" for bypass regions is retired.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use super::heap4::Heap4;

/// The simulator's allocator instance.
///
/// The `#[global_allocator]` *attribute* is only legal in a binary crate, so
/// the platform registers a newtype that forwards here —
/// [`crate::declare_sim_global_allocator`] writes it. The instance itself
/// lives beside the accounting functions because they all reach it directly.
pub static GLOBAL: CappedAllocator = CappedAllocator::new();

/// Declare the binary crate's `#[global_allocator]` as a forwarder to
/// [`GLOBAL`].
///
/// A `#[global_allocator]` must be a `static` of the binary crate, so the
/// instance cannot simply be this crate's; every family would otherwise copy
/// the same forwarding newtype. All four methods are forwarded rather than
/// leaning on the trait defaults: the defaults would re-enter the newtype,
/// which happens to be equivalent today only because `CappedAllocator` also
/// takes them — forwarding explicitly keeps that a non-question if it ever
/// overrides `realloc`. Unarmed, the allocator is a pass-through to the
/// system one, so a plain host test build is unaffected.
///
/// ```ignore
/// #[cfg(any(test, feature = "sim"))]
/// picodroid_core::declare_sim_global_allocator!();
/// ```
#[macro_export]
macro_rules! declare_sim_global_allocator {
    () => {
        /// Routes every allocation to the simulator's capped allocator in
        /// picodroid-core.
        struct SimGlobalAlloc;

        // SAFETY: a stateless forwarder: every method hands its arguments
        // unchanged to `GLOBAL`, whose `GlobalAlloc` impl upholds the trait.
        unsafe impl ::core::alloc::GlobalAlloc for SimGlobalAlloc {
            unsafe fn alloc(&self, layout: ::core::alloc::Layout) -> *mut u8 {
                ::core::alloc::GlobalAlloc::alloc(&$crate::hal::sim::allocator::GLOBAL, layout)
            }
            unsafe fn dealloc(&self, ptr: *mut u8, layout: ::core::alloc::Layout) {
                ::core::alloc::GlobalAlloc::dealloc(
                    &$crate::hal::sim::allocator::GLOBAL,
                    ptr,
                    layout,
                )
            }
            unsafe fn realloc(
                &self,
                ptr: *mut u8,
                layout: ::core::alloc::Layout,
                new_size: usize,
            ) -> *mut u8 {
                ::core::alloc::GlobalAlloc::realloc(
                    &$crate::hal::sim::allocator::GLOBAL,
                    ptr,
                    layout,
                    new_size,
                )
            }
            unsafe fn alloc_zeroed(&self, layout: ::core::alloc::Layout) -> *mut u8 {
                ::core::alloc::GlobalAlloc::alloc_zeroed(
                    &$crate::hal::sim::allocator::GLOBAL,
                    layout,
                )
            }
        }

        #[global_allocator]
        static SIM_GLOBAL: SimGlobalAlloc = SimGlobalAlloc;
    };
}

thread_local! {
    /// Nonzero while the current thread is inside a [`bypass`] region. Reads and
    /// writes must never re-enter the global allocator, so this is
    /// `const`-initialized — a lazily-initialized TLS would allocate on first
    /// touch and deadlock inside `alloc`. `Cell<u32>` has no destructor, so it
    /// also stays accessible during thread teardown.
    static BYPASS_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// RAII guard returned by [`bypass`]. While at least one guard is alive on the
/// current thread, `CappedAllocator` serves allocations straight from the host
/// allocator, uncounted and uncapped.
///
/// This exists to exclude *host-only* artifacts — allocations that have no
/// counterpart in the MCU JVM heap — from the simulated budget. The motivating
/// case is the minifb window's full-screen backing buffer: at `Scale::X2` a
/// 240×240 display becomes a 480×480 `Vec<u32>` (≈900 KB), which dwarfs a
/// realistic MCU heap and would OOM the sim before the app runs. On real
/// hardware there is no such framebuffer — LVGL renders into a small banded
/// buffer streamed to the panel over SPI — so charging it to the JVM heap cap
/// mismodels the device. The flash-modeled APK load (docs/parity-audit.md
/// APK-01) and host `std::thread` spawn internals get the same treatment.
///
/// Frees route by pointer range, not by bypass state: freeing an arena
/// pointer inside a bypass region (or a bypassed pointer outside one) does
/// the right thing automatically.
#[must_use]
pub struct BypassGuard(());

impl Drop for BypassGuard {
    fn drop(&mut self) {
        bypass_exit();
    }
}

/// Enter a heap-cap bypass region on the current thread until the returned
/// guard is dropped. See [`BypassGuard`]. Nesting is supported (depth-counted).
pub fn bypass() -> BypassGuard {
    bypass_enter();
    BypassGuard(())
}

/// Unguarded half of [`bypass`], for callers that cannot hold an RAII guard.
///
/// This exists for `glue.rs`'s `PlatformHooks` impl: the shared crate's
/// bypass guard lives in `picodroid-core`, so the seam is a pair of enter/
/// exit calls rather than a guard handed across the crate boundary. Every
/// `bypass_enter` must be matched by exactly one [`bypass_exit`]; prefer
/// [`bypass`] whenever a guard will do.
pub fn bypass_enter() {
    BYPASS_DEPTH.with(|d| d.set(d.get() + 1));
}

/// Leave a region opened by [`bypass_enter`]. Saturating, so an unmatched
/// exit cannot underflow the depth and silently disable accounting.
pub fn bypass_exit() {
    BYPASS_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
}

fn bypass_active() -> bool {
    BYPASS_DEPTH.with(Cell::get) != 0
}

/// Start simulated-heap accounting. Called first thing in sim `main`, the
/// moment that corresponds to the device's entry point: everything after
/// this (fs::init, JVM boot, app) is charged like the device's FreeRTOS
/// heap; everything before it is host-runtime noise with no device analog.
pub fn arm() {
    GLOBAL.armed.store(true, Ordering::Release);
    // Force stdout's line buffer into existence under bypass so the first
    // real println doesn't charge a host BufWriter to the simulated heap.
    let _b = bypass();
    let (_, _, limit) = GLOBAL.heap_stats();
    if limit == usize::MAX {
        println!("[sim] heap: UNCAPPED (limit 0) — device heap model disabled");
    } else {
        println!(
            "[sim] heap: heap_4 arena {} KB (device model; PICODROID_HEAP_LIMIT_KB or sim.sh -l overrides, 0 = uncapped)",
            limit / 1024
        );
    }
}

/// Snapshot the global allocator. Returns `(current_bytes, peak_bytes, limit_bytes)`.
/// In arena mode "current" is `arena_size - free` and "peak" is
/// `arena_size - min_ever_free` — the device-comparable figures.
pub fn heap_stats() -> (usize, usize, usize) {
    GLOBAL.heap_stats()
}

/// Full heap_4 statistics (free list walk included), when the arena is
/// active. The payload for parity checkpoints — directly comparable with
/// the device's `vPortGetHeapStats`/sysmon output.
#[allow(dead_code)] // harness surface: consumed by parity heap-snapshot lanes
pub fn heap4_stats() -> Option<super::heap4::HeapStats> {
    let base = GLOBAL.arena_base.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    let guard = GLOBAL
        .arena
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.as_ref().map(|h| h.stats())
}

/// Per-phase heap checkpoint for the sim. Prints the byte delta since the
/// previous call alongside the current and peak values so transient bloat
/// (allocations freed before the next checkpoint) is visible too.
pub fn checkpoint(label: &str) {
    static PREV: AtomicUsize = AtomicUsize::new(0);
    let (cur, peak, _) = heap_stats();
    let prev = PREV.swap(cur, Ordering::Relaxed);
    let delta = cur as i64 - prev as i64;
    println!(
        "[sim] heap phase: {:<22} delta {:+8} B (cur {:>8} B, peak {:>8} B)",
        label, delta, cur, peak,
    );
}

pub struct CappedAllocator {
    /// Byte meter for uncapped mode (limit 0 / `usize::MAX`).
    allocated: AtomicUsize,
    peak: AtomicUsize,
    /// Accounting starts only once [`arm`] flips this; host-runtime
    /// allocations before sim `main` have no device counterpart.
    armed: AtomicBool,
    /// The heap_4 arena (capped mode). Lock = the port's stand-in for
    /// `vTaskSuspendAll`. NEVER allocate while holding it.
    arena: Mutex<Option<Heap4>>,
    /// Arena bounds for the lock-free `dealloc` range check. 0 = no arena.
    arena_base: AtomicUsize,
    arena_len: AtomicUsize,
    /// Test-only limit override (0 = consult the environment).
    limit_override: AtomicUsize,
}

impl Default for CappedAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl CappedAllocator {
    /// `const` because [`GLOBAL`] is a `static`: an allocator cannot be
    /// initialized lazily, since the first allocation may arrive before any
    /// initializer could have run.
    pub const fn new() -> Self {
        Self {
            allocated: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            armed: AtomicBool::new(false),
            arena: Mutex::new(None),
            arena_base: AtomicUsize::new(0),
            arena_len: AtomicUsize::new(0),
            limit_override: AtomicUsize::new(0),
        }
    }

    fn limit(&self) -> usize {
        let ov = self.limit_override.load(Ordering::Relaxed);
        if ov != 0 {
            ov
        } else {
            heap_limit()
        }
    }

    #[cfg(test)]
    fn set_limit_for_test(&self, bytes: usize) {
        self.limit_override.store(bytes, Ordering::Relaxed);
    }
    #[cfg(test)]
    fn arm_for_test(&self) {
        self.armed.store(true, Ordering::Release);
    }

    /// Returns (current_bytes, peak_bytes, limit_bytes).
    pub fn heap_stats(&self) -> (usize, usize, usize) {
        let base = self.arena_base.load(Ordering::Acquire);
        if base != 0 {
            let guard = self
                .arena
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(h) = guard.as_ref() {
                let size = h.arena_size() as usize;
                return (
                    size - h.free_bytes() as usize,
                    size - h.min_ever_free_bytes() as usize,
                    size,
                );
            }
        }
        (
            self.allocated.load(Ordering::Relaxed),
            self.peak.load(Ordering::Relaxed),
            self.limit(),
        )
    }

    /// Emit a message through a no-alloc path and abort. For failures inside
    /// the allocator itself, where panicking (which allocates) would either
    /// recurse or deadlock on the arena lock.
    fn die(msg: &str) -> ! {
        raw_stderr(msg.as_bytes());
        std::process::abort();
    }
}

/// Write `bytes` to stderr with one `write(2)` call: no `std::io` buffer, no
/// lock and no allocation, so it is usable inside the global allocator. A
/// short or failed write is ignored, as every caller is about to abort or
/// return a null allocation anyway.
pub(super) fn raw_stderr(bytes: &[u8]) {
    // SAFETY: `bytes` is a live slice, so `write` reads exactly `bytes.len()`
    // initialised bytes from valid memory; fd 2 needs no further invariant
    // (a closed descriptor makes `write` fail with EBADF, which is ignored).
    unsafe {
        libc::write(2, bytes.as_ptr().cast(), bytes.len());
    }
}

/// Look an environment variable up without allocating and hand its bytes to
/// `f`; `None` when it is unset. `std::env::var` allocates a `String`, which
/// inside the global allocator would re-enter it and deadlock on the arena
/// lock, so the allocator and the site ledger read the environment this way.
pub(super) fn with_env<R>(name: &CStr, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    // `getenv` returns NULL or a pointer to a NUL-terminated string inside
    // the process environment, valid until the environment is modified, and
    // nothing in this code base modifies it (no `set_var`, `remove_var`,
    // `setenv` or `putenv` anywhere in the tree).
    // SAFETY: `name` is NUL-terminated by type; the pointer is valid as
    // above, and the borrow handed to `f` ends before this returns.
    let value = unsafe {
        let p = libc::getenv(name.as_ptr());
        if p.is_null() {
            return None;
        }
        CStr::from_ptr(p)
    };
    Some(f(value.to_bytes()))
}

/// Read the heap limit once per process. Defaults to [`DEVICE_HEAP_BYTES`]
/// when `PICODROID_HEAP_LIMIT_KB` is unset; `0` means uncapped.
///
/// Uses libc `getenv` instead of `std::env::var` to avoid allocating a String
/// inside the global allocator (which would cause reentrant deadlock).
fn heap_limit() -> usize {
    use std::sync::atomic::AtomicUsize;
    // 0 = not yet initialized, 1..MAX = limit, MAX = unlimited.
    static LIMIT: AtomicUsize = AtomicUsize::new(0);

    let cached = LIMIT.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }

    let val = parse_env_limit();
    LIMIT.store(val, Ordering::Relaxed);
    val
}

// Device heap arena size (FreeRTOS `configTOTAL_HEAP_SIZE`) for the MCU being
// simulated — the default heap cap. Generated from the MCU toml's `heap_kb`,
// the same single source that feeds the FreeRTOS C build, so the two can
// never drift (docs/parity-audit.md M2).
use crate::board_cfg::heap::DEVICE_HEAP_BYTES;

/// Parse `PICODROID_HEAP_LIMIT_KB` from the environment without allocating.
/// Unset or unparseable → the chip's device heap size; `0` → uncapped.
fn parse_env_limit() -> usize {
    let kb = with_env(c"PICODROID_HEAP_LIMIT_KB", |v| {
        core::str::from_utf8(v)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
    });
    match kb.flatten() {
        Some(0) => usize::MAX,
        Some(kb) => kb * 1024,
        None => DEVICE_HEAP_BYTES,
    }
}

/// Whether offensive-mode trailing canaries are on
/// (`PICODROID_MEMDIAG_OFFENSIVE=1`). Every arena allocation gets 4 extra
/// bytes holding a magic word right past the requested size, verified on
/// free — catches payload overruns (LVGL C code included) that the byte
/// counter and heap_4 header checks cannot see. Read once via libc::getenv:
/// this runs inside the allocator, where std::env (which allocates) would
/// recurse. Opt-in only — canaries shift block sizes, so byte-exact parity
/// runs must leave this off.
#[cfg(feature = "mem-diag")]
fn canaries_enabled() -> bool {
    // 0 = unread, 1 = off, 2 = on.
    static STATE: AtomicUsize = AtomicUsize::new(0);
    match STATE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = with_env(c"PICODROID_MEMDIAG_OFFENSIVE", |v| {
                matches!(v, b"1" | b"on" | b"true" | b"yes")
            })
            .unwrap_or(false);
            STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

/// Canary magic written past each arena allocation in offensive mode.
#[cfg(feature = "mem-diag")]
const CANARY: u32 = 0xDEAD_C0DE;

/// Stack-only writer for emitting OOM diagnostics without re-entering the
/// allocator (println/eprintln would recurse since the failing allocation
/// arrives via the global allocator).
struct StackWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}
impl core::fmt::Write for StackWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        let n = b.len().min(self.buf.len().saturating_sub(self.pos));
        self.buf[self.pos..self.pos + n].copy_from_slice(&b[..n]);
        self.pos += n;
        Ok(())
    }
}

// Blocks come from `System` (bypass, pre-arm, uncapped) or from the heap_4
// arena, whose blocks are 8-aligned and which aborts on an `align > 8`
// request; `dealloc` routes by address.
// SAFETY: so `alloc` returns null or a block fit for `layout` that nothing
// else uses until `dealloc` hands it back to the allocator that made it.
unsafe impl GlobalAlloc for CappedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Host-only allocation (see `bypass`): serve it uncounted and uncapped.
        if bypass_active() {
            // SAFETY: the caller's `GlobalAlloc::alloc` contract (a non-zero
            // size).
            return unsafe { System.alloc(layout) };
        }
        // Pre-arm: host runtime startup, no device counterpart.
        if !self.armed.load(Ordering::Acquire) {
            // SAFETY: the caller's `GlobalAlloc::alloc` contract (a non-zero
            // size).
            return unsafe { System.alloc(layout) };
        }

        let size = layout.size();
        let limit = self.limit();

        if limit == usize::MAX {
            // Uncapped mode: plain byte metering over the host allocator.
            let prev = self.allocated.fetch_add(size, Ordering::Relaxed);
            // SAFETY: the caller's `GlobalAlloc::alloc` contract (a non-zero
            // size).
            let ptr = unsafe { System.alloc(layout) };
            if ptr.is_null() {
                self.allocated.fetch_sub(size, Ordering::Relaxed);
            } else {
                self.peak.fetch_max(prev + size, Ordering::Relaxed);
            }
            return ptr;
        }

        // Arena mode. The device allocator ignores Layout::align entirely
        // (freertos-rust passes only the size; heap_4 returns 8-aligned), so
        // an over-aligned allocation would be MISALIGNED on hardware. Abort
        // loudly instead of silently diverging — this is a tripwire for a
        // latent device bug (docs/parity-audit.md MEM-05). V5 measured zero
        // such allocations in current workloads.
        if layout.align() > 8 {
            Self::die(
                "[sim] FATAL: allocation with align > 8 — freertos-rust drops \
                 Layout::align, so this allocation would be misaligned on real \
                 hardware (docs/parity-audit.md MEM-05)\n",
            );
        }
        let Ok(want) = u32::try_from(size) else {
            return std::ptr::null_mut(); // > 4 GB cannot fit a 32-bit arena
        };
        // Offensive mode: reserve room for the trailing canary word.
        #[cfg(feature = "mem-diag")]
        let want = if canaries_enabled() {
            match want.checked_add(4) {
                Some(w) => w,
                None => return std::ptr::null_mut(),
            }
        } else {
            want
        };

        let mut guard = self
            .arena
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_none() {
            // First armed capped allocation: materialize the arena.
            if limit > u32::MAX as usize - 16 {
                Self::die(
                    "[sim] FATAL: heap limit too large for the 32-bit heap_4 \
                     arena; use -l 0 for an uncapped run\n",
                );
            }
            let arena_layout = Layout::from_size_align(limit, 8).unwrap();
            // SAFETY: `limit` is non-zero (`heap_limit` maps 0 to uncapped,
            // handled above), so the layout has a non-zero size.
            let base = unsafe { System.alloc(arena_layout) };
            if base.is_null() {
                Self::die("[sim] FATAL: host allocation of the sim arena failed\n");
            }
            // SAFETY: `base` is a fresh 8-aligned host block of `limit` bytes
            // that is never freed and that only this arena uses.
            *guard = Some(unsafe { Heap4::init(base, limit as u32) });
            self.arena_len.store(limit, Ordering::Relaxed);
            self.arena_base.store(base as usize, Ordering::Release);
        }
        let heap = guard.as_mut().unwrap();
        let p = match heap.malloc(want) {
            Some(off) => {
                let base = self.arena_base.load(Ordering::Relaxed);
                // SAFETY: `off` is a payload offset `malloc` returned, inside
                // the arena of `arena_len` bytes at `base`.
                let p = unsafe { (base as *mut u8).add(off as usize) };
                #[cfg(feature = "mem-diag")]
                if canaries_enabled() {
                    // Trailing canary right past the requested size; the
                    // matching check runs in `dealloc` before the free.
                    // SAFETY: offensive mode asked `malloc` for `size + 4`
                    // bytes, so the four canary bytes past `size` are inside
                    // this block.
                    unsafe { core::ptr::write_unaligned(p.add(size) as *mut u32, CANARY) };
                }
                p
            }
            None => {
                // Emergency-GC path upstream (Rust try_reserve → Err → GC),
                // exactly as on device. Emit heap_4 diagnostics: "free but
                // fragmented" is the interesting case a byte counter can't
                // report.
                let s = heap.stats();
                let mut buf = [0u8; 256];
                let pos = {
                    let mut w = StackWriter {
                        buf: &mut buf,
                        pos: 0,
                    };
                    use core::fmt::Write;
                    let _ = writeln!(
                        w,
                        "[sim] OOM: tried {} B — free {} B, largest block {} B, \
                         {} free blocks, min-ever-free {} B, arena {} B",
                        size,
                        s.free_bytes,
                        s.largest_free_block,
                        s.free_blocks,
                        s.min_ever_free_bytes,
                        heap.arena_size(),
                    );
                    w.pos
                };
                raw_stderr(&buf[..pos]);
                std::ptr::null_mut()
            }
        };
        // The site ledger takes its own lock and unwinds the stack: never
        // while the arena lock is held.
        drop(guard);
        #[cfg(feature = "mem-diag")]
        if !p.is_null() {
            super::alloc_ledger::record(p, size);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Arena pointers are recognized by range, independent of bypass
        // state or arming — the only correct routing for a pointer that
        // lives inside the arena.
        let base = self.arena_base.load(Ordering::Acquire);
        if base != 0 {
            let addr = ptr as usize;
            let len = self.arena_len.load(Ordering::Relaxed);
            if addr >= base && addr < base + len {
                #[cfg(feature = "mem-diag")]
                super::alloc_ledger::forget(ptr);
                #[cfg(feature = "mem-diag")]
                if canaries_enabled() {
                    // SAFETY: an arena block allocated with `layout` (the
                    // `dealloc` contract) while canaries were on, so it has
                    // four canary bytes past `layout.size()`.
                    let c = unsafe {
                        core::ptr::read_unaligned(
                            (ptr as *const u8).add(layout.size()) as *const u32
                        )
                    };
                    if c != CANARY {
                        // No-alloc report + abort: the allocator lock isn't
                        // held yet and println would recurse into alloc.
                        let msg = b"[sim] FATAL: heap canary smashed - buffer overrun past \
                                    allocation end (mem-diag offensive mode)\n";
                        raw_stderr(msg);
                        std::process::abort();
                    }
                }
                let mut guard = self
                    .arena
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(heap) = guard.as_mut() {
                    // SAFETY: `ptr` is inside the arena, so `alloc` got it
                    // from this heap's `malloc` with `layout`, and the
                    // `GlobalAlloc::dealloc` contract frees it once.
                    let freed = unsafe { heap.free((addr - base) as u32) };
                    if let Err(msg) = freed {
                        drop(guard);
                        raw_stderr(msg.as_bytes());
                        raw_stderr(b"\n");
                        std::process::abort();
                    }
                }
                return;
            }
        }
        // SAFETY: `ptr` is outside the arena, so `alloc` got it from `System`
        // with `layout` (the caller's `GlobalAlloc::dealloc` contract).
        unsafe { System.dealloc(ptr, layout) };
        if bypass_active() || !self.armed.load(Ordering::Acquire) {
            return;
        }
        // Uncapped-mode meter. `fetch_update` saturates at zero as a
        // backstop (e.g. a pre-arm allocation freed after arming).
        let size = layout.size();
        let _ = self
            .allocated
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                Some(cur.saturating_sub(size))
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block handed out by [`alloc`], owning the pointer until [`free`]
    /// consumes it. Only `alloc` builds one, so `free` always returns a
    /// pointer to the allocator that produced it, with its own layout.
    struct Block<'a> {
        from: &'a CappedAllocator,
        ptr: *mut u8,
        layout: Layout,
    }

    /// `GlobalAlloc::alloc` for the tests; `None` is the allocator's null.
    fn alloc(from: &CappedAllocator, layout: Layout) -> Option<Block<'_>> {
        assert_ne!(layout.size(), 0, "GlobalAlloc::alloc needs a non-zero size");
        // SAFETY: the layout's size is non-zero (asserted above), the only
        // precondition of `GlobalAlloc::alloc`.
        let ptr = unsafe { GlobalAlloc::alloc(from, layout) };
        (!ptr.is_null()).then_some(Block { from, ptr, layout })
    }

    /// `GlobalAlloc::dealloc` of a block from [`alloc`].
    fn free(b: Block<'_>) {
        // Only `alloc` builds a `Block`, from a non-null pointer `b.from`
        // returned for `b.layout`, and `free` consumes it.
        // SAFETY: so the pointer is live, allocated by this allocator with
        // this layout, and freed exactly once.
        unsafe { GlobalAlloc::dealloc(b.from, b.ptr, b.layout) };
    }

    /// Uncapped-mode metering (legacy counter behavior behind `-l 0`).
    #[test]
    fn uncapped_tracking_accuracy() {
        let heap = CappedAllocator::new();
        heap.set_limit_for_test(usize::MAX);
        heap.arm_for_test();
        let layout = Layout::from_size_align(256, 8).unwrap();

        let b = alloc(&heap, layout).expect("alloc");
        let (current, peak, _) = heap.heap_stats();
        assert_eq!(current, 256);
        assert_eq!(peak, 256);

        free(b);
        let (current, peak, _) = heap.heap_stats();
        assert_eq!(current, 0);
        assert_eq!(peak, 256); // peak unchanged
    }

    #[test]
    fn unarmed_allocations_are_not_counted() {
        let heap = CappedAllocator::new();
        heap.set_limit_for_test(usize::MAX);
        let layout = Layout::from_size_align(512, 8).unwrap();
        let b = alloc(&heap, layout).expect("alloc");
        assert_eq!(
            heap.heap_stats().0,
            0,
            "pre-arm allocs have no device analog"
        );
        free(b);
        assert_eq!(heap.heap_stats().0, 0);
    }

    /// Capped mode is a real heap_4 arena: device header costs, real OOM,
    /// and device-comparable stats.
    #[test]
    fn arena_mode_charges_heap4_costs_and_ooms() {
        let heap = CappedAllocator::new();
        heap.set_limit_for_test(64 * 1024);
        heap.arm_for_test();

        // The pxEnd marker occupies 8 B of the arena forever — the device's
        // `configTOTAL_HEAP_SIZE - xPortGetFreeHeapSize` includes it too.
        const PXEND: usize = 8;

        let layout = Layout::from_size_align(1, 1).unwrap();
        let p = alloc(&heap, layout).expect("alloc");
        let (current, _, limit) = heap.heap_stats();
        assert_eq!(limit, 64 * 1024);
        assert_eq!(
            current,
            16 + PXEND,
            "1-byte alloc costs a 16 B heap_4 block (plus the pxEnd marker)"
        );

        free(p);
        assert_eq!(heap.heap_stats().0, PXEND);

        // Exhaust the arena → null, like pvPortMalloc.
        let big = Layout::from_size_align(60 * 1024, 8).unwrap();
        let a = alloc(&heap, big).expect("first 60 KB fits");
        assert!(
            alloc(&heap, big).is_none(),
            "second 60 KB cannot fit a 64 KB arena"
        );
        free(a);
        let c = alloc(&heap, big).expect("freed arena space is reusable");
        free(c);
    }

    #[test]
    fn bypass_allocations_stay_off_the_arena() {
        let heap = CappedAllocator::new();
        heap.set_limit_for_test(64 * 1024);
        heap.arm_for_test();
        let layout = Layout::from_size_align(4096, 8).unwrap();

        // Trigger arena creation with one small counted alloc.
        let counted = alloc(&heap, layout).expect("alloc");
        let used_before = heap.heap_stats().0;

        let p = {
            let _bypass = bypass();
            alloc(&heap, layout).expect("bypassed alloc")
        };
        assert_eq!(
            heap.heap_stats().0,
            used_before,
            "bypassed alloc is uncounted"
        );

        // Freed OUTSIDE the bypass region: pointer-range routing sends it to
        // the host allocator, and the arena books stay balanced. (The old
        // "balance rule" made this an accounting bug; now it's just correct.)
        free(p);
        assert_eq!(heap.heap_stats().0, used_before);

        // And an arena pointer freed INSIDE a bypass region still returns to
        // the arena. Only the 8 B pxEnd marker remains occupied.
        {
            let _bypass = bypass();
            free(counted);
        }
        assert_eq!(heap.heap_stats().0, 8);
    }
}
