// SPDX-License-Identifier: GPL-3.0-only
//! When memory counts as short, and the once-per-episode arms behind the
//! app's `onTrimMemory` signal (`lifecycle::trim_memory` delivers it).
//!
//! Two arenas, two thresholds: the LVGL pool below 1/[`POOL_PRESSURE_DIVISOR`]
//! free — a covered Activity's view tree is parked there — and the native
//! heap below 1/[`HEAP_PRESSURE_DIVISOR`] free, the heap the JVM's object
//! storage grows in. The activity stack reclaims covered Activities past
//! either line (`lifecycle::activity_stack`); the event loop tells the app
//! once per drop below it, re-arming only once the measure has risen a band
//! above the line, so an app that stays low is not told every frame. This
//! module is the pure part, unit-tested; the lifecycle module (compiled out
//! of the host test binary) reads the arenas and makes the upcall.

#[cfg(feature = "sim")]
use core::sync::atomic::AtomicI32;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

/// Reclaim below this much of the LVGL pool free (1/N of its size).
pub const POOL_PRESSURE_DIVISOR: usize = 8;
/// Reclaim below this much of the native heap free (1/N of its size).
pub const HEAP_PRESSURE_DIVISOR: usize = 16;

/// `ComponentCallbacks2.TRIM_MEMORY_RUNNING_CRITICAL`. Keep in step with
/// `sdk/java/picodroid/content/ComponentCallbacks2.java`.
pub const RUNNING_CRITICAL: i32 = 15;
/// What `Context.fireTrimMemory` reads as `onLowMemory` (`Context.LOW_MEMORY`).
pub const LOW_MEMORY: i32 = -1;

/// The pool is walked on every Nth frame tick: `lv_mem_monitor` visits
/// every block, so not every frame.
pub const POOL_POLL_TICKS: u8 = 8;

/// The once-per-episode state of the two arms, plus the native total the
/// thresholds are taken from. One per process (the loop and the push path
/// share it, so a launch-time delivery is not repeated by the next frame);
/// everything runs on the main task, so `Relaxed` is enough.
pub struct Episode {
    native_armed: AtomicBool,
    pool_armed: AtomicBool,
    native_total: AtomicUsize,
    ticks: AtomicU8,
}

impl Episode {
    pub const fn new() -> Self {
        Self {
            native_armed: AtomicBool::new(true),
            pool_armed: AtomicBool::new(true),
            native_total: AtomicUsize::new(0),
            ticks: AtomicU8::new(0),
        }
    }

    /// Back to armed, for the next app of a run.
    pub fn reset(&self) {
        self.native_armed.store(true, Ordering::Relaxed);
        self.pool_armed.store(true, Ordering::Relaxed);
        self.ticks.store(0, Ordering::Relaxed);
    }

    /// A delivery happened elsewhere (the push path): neither arm repeats
    /// it until its measure has recovered.
    pub fn disarm(&self) {
        self.native_armed.store(false, Ordering::Relaxed);
        self.pool_armed.store(false, Ordering::Relaxed);
    }

    /// Feed the native arm `free` of `total` bytes: true exactly once per
    /// drop below `total / HEAP_PRESSURE_DIVISOR`; it re-arms once free is
    /// a further `total / 64` above that line. `total == 0` or an unbounded
    /// `free` (the uncapped simulator) never fires.
    pub fn observe_native(&self, free: usize, total: usize) -> bool {
        Self::observe(&self.native_armed, free, total, HEAP_PRESSURE_DIVISOR, 64)
    }

    /// The pool arm: fires below `total / POOL_PRESSURE_DIVISOR`, re-arms a
    /// `total / 32` band above it.
    pub fn observe_pool(&self, free: usize, total: usize) -> bool {
        Self::observe(&self.pool_armed, free, total, POOL_PRESSURE_DIVISOR, 32)
    }

    fn observe(
        armed: &AtomicBool,
        free: usize,
        total: usize,
        divisor: usize,
        band_divisor: usize,
    ) -> bool {
        if total == 0 || free == usize::MAX {
            return false;
        }
        let low = total / divisor;
        if armed.load(Ordering::Relaxed) {
            if free < low {
                armed.store(false, Ordering::Relaxed);
                return true;
            }
        } else if free > low.saturating_add(total / band_divisor) {
            armed.store(true, Ordering::Relaxed);
        }
        false
    }

    /// The native arena's size, read once through `read_total` (a free-list
    /// walk) and kept: the heap does not grow.
    pub fn native_total(&self, read_total: impl FnOnce() -> usize) -> usize {
        let total = self.native_total.load(Ordering::Relaxed);
        if total != 0 {
            return total;
        }
        let total = read_total();
        self.native_total.store(total, Ordering::Relaxed);
        total
    }

    /// Whether this frame tick is a pool-polling one.
    pub fn pool_due(&self, tick: bool) -> bool {
        if !tick {
            return false;
        }
        let n = self.ticks.load(Ordering::Relaxed).wrapping_add(1);
        self.ticks.store(n, Ordering::Relaxed);
        n.is_multiple_of(POOL_POLL_TICKS)
    }
}

impl Default for Episode {
    fn default() -> Self {
        Self::new()
    }
}

/// The process's one episode state.
pub static EPISODE: Episode = Episode::new();

/// No synthetic level pending.
#[cfg(feature = "sim")]
const NONE: i32 = i32::MIN;

/// A level the simulator's control channel asked for (`memtrim <level>`),
/// delivered by the event loop's next poll without any pressure behind it.
#[cfg(feature = "sim")]
static SYNTHETIC: AtomicI32 = AtomicI32::new(NONE);

/// `memtrim <level>` from the control channel: queue one delivery.
#[cfg(feature = "sim")]
pub fn request_synthetic(level: i32) {
    SYNTHETIC.store(level, Ordering::Relaxed);
}

/// The queued synthetic level, if any, cleared.
#[cfg(feature = "sim")]
pub fn take_synthetic() -> Option<i32> {
    let level = SYNTHETIC.swap(NONE, Ordering::Relaxed);
    (level != NONE).then_some(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_arm_fires_once_and_rearms_a_band_above() {
        let ep = Episode::new();
        let total = 416 * 1024;
        let low = total / HEAP_PRESSURE_DIVISOR;
        assert!(
            !ep.observe_native(low, total),
            "at the line is not below it"
        );
        assert!(ep.observe_native(low - 1, total), "first drop fires");
        assert!(!ep.observe_native(low - 1, total), "staying low is silent");
        assert!(!ep.observe_native(1, total));
        assert!(
            !ep.observe_native(low + total / 64, total),
            "the band's edge does not re-arm"
        );
        assert!(
            !ep.observe_native(low + total / 64 + 1, total),
            "re-arming fires nothing"
        );
        assert!(
            ep.observe_native(low - 1, total),
            "the next drop fires again"
        );
    }

    #[test]
    fn pool_arm_uses_the_pool_divisor_and_band() {
        let ep = Episode::new();
        let total = 48 * 1024;
        let low = total / POOL_PRESSURE_DIVISOR;
        assert!(ep.observe_pool(low - 1, total));
        assert!(!ep.observe_pool(low + total / 32, total));
        assert!(!ep.observe_pool(low + total / 32 + 1, total));
        assert!(ep.observe_pool(0, total));
    }

    #[test]
    fn unknown_sizes_never_fire() {
        let ep = Episode::new();
        assert!(!ep.observe_native(0, 0));
        assert!(
            !ep.observe_native(usize::MAX, usize::MAX),
            "the uncapped simulator"
        );
        assert!(!ep.observe_pool(0, 0), "no pool: a main-class app");
    }

    #[test]
    fn disarm_and_reset() {
        let ep = Episode::new();
        let total = 1 << 20;
        ep.disarm();
        assert!(
            !ep.observe_native(0, total),
            "a push-path delivery is not repeated"
        );
        assert!(!ep.observe_pool(0, total));
        ep.reset();
        assert!(ep.observe_native(0, total));
        assert!(ep.observe_pool(0, total));
    }

    #[test]
    fn pool_is_polled_every_nth_tick_only() {
        let ep = Episode::new();
        assert!(!ep.pool_due(false));
        let due: u32 = (0..u32::from(POOL_POLL_TICKS) * 3)
            .map(|_| u32::from(ep.pool_due(true)))
            .sum();
        assert_eq!(due, 3);
    }

    #[test]
    fn native_total_is_read_once() {
        let ep = Episode::new();
        let mut reads = 0;
        let mut read = || {
            reads += 1;
            4096
        };
        assert_eq!(ep.native_total(&mut read), 4096);
        assert_eq!(ep.native_total(&mut read), 4096);
        assert_eq!(reads, 1);
    }

    #[test]
    fn levels_match_the_java_interface() {
        assert_eq!(RUNNING_CRITICAL, 15);
        assert!(LOW_MEMORY < 0);
    }
}
