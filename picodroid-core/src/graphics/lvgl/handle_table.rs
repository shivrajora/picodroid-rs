// SPDX-License-Identifier: GPL-3.0-only
//! LVGL object handle ↔ Java `nativeHandle` (i32) conversion.
//!
//! One width-independent **generation-tagged table** (audit P1-9 /
//! parity-audit HAL-05): a handle encodes a slot index in its low bits and
//! that slot's generation above them, so a deleted widget's handle goes
//! stale the moment its `LV_EVENT_DELETE` hook fires — `lookup` returns
//! null instead of dangling into freed LVGL memory. Slots are reused via a
//! free list; reuse bumps the generation, so a stale handle can only
//! false-validate after 65,536 reuses of the same slot while the stale
//! Java reference is still held (accepted as negligible).
//!
//! Encoding (`0` stays `Handle::NULL`, sign bit never set so
//! `Handle::from_java`'s `<= 0 → NULL` keeps working):
//!
//! ```text
//! bits 0..INDEX_BITS   slot index   (SLOTS = 256 device / 1024 host)
//! bits INDEX_BITS..+16 generation   (starts at 1, skips 0 on wrap)
//! remaining high bits  0
//! ```
//!
//! Validation is by full re-encode equality — an id whose high bits carry
//! junk can never equal `encode(current_gen, idx)`, so forged/corrupt ids
//! are rejected, not truncated into a live slot.
//!
//! **Staging:** on 32-bit targets the legacy zero-cost cast
//! (`ptr as u32 as i32`, no invalidation — the dangle documented as
//! HAL-05/S1) remains the default until the `handle-table-32` feature is
//! made default after a HIL soak; 64-bit always uses the table. The
//! sim's `PICODROID_HANDLE_SANITIZER` (default-on in `scripts/sim.sh`)
//! turns any stale lookup into a loud abort with a backtrace.

#[cfg(not(test))]
use crate::lvgl_ffi::lv_obj_t;

// FFI seam for `cargo test`: the table logic is pure and pointer-width
// independent; only the delete-hook *installation* touches LVGL, and that
// path is exercised end-to-end by the sim suite. Under test, `lv_obj_t` is
// an opaque local type and tests drive invalidation directly.
#[cfg(test)]
#[allow(non_camel_case_types)]
pub enum lv_obj_t {}

// ── Legacy 32-bit cast (no invalidation) — until `handle-table-32` flips ────

#[cfg(all(target_pointer_width = "32", not(feature = "handle-table-32")))]
mod imp {
    use super::lv_obj_t;

    /// Store a pointer as a Java `nativeHandle`: bit-preserving cast.
    /// A deleted handle *dangles* (see module note) — the generational
    /// table behind `handle-table-32` is the fix being staged in.
    #[inline(always)]
    pub fn register(ptr: *mut lv_obj_t) -> i32 {
        ptr as u32 as i32
    }

    /// Same cast as [`register`] — the screen is not tracked separately.
    #[inline(always)]
    pub fn register_pinned(ptr: *mut lv_obj_t) -> i32 {
        register(ptr)
    }

    /// Recover a pointer from a Java `nativeHandle`: bit-preserving cast.
    #[inline(always)]
    pub fn lookup(id: i32) -> *mut lv_obj_t {
        id as u32 as *mut lv_obj_t
    }

    /// No-op — there is no table to clear.
    #[inline(always)]
    pub fn reset() {}
}

// ── Generation-tagged table (64-bit always; 32-bit with `handle-table-32`) ──

#[cfg(any(not(target_pointer_width = "32"), feature = "handle-table-32"))]
mod imp {
    use super::lv_obj_t;
    use core::ptr::null_mut;

    // Device slot count is board-tunable (`handle_slots` in board.toml,
    // default 256); the host is fixed at 1024 so `graphicsbench`-scale churn
    // (measured peak: 43 live widgets) never brushes the ceiling.
    #[cfg(target_pointer_width = "32")]
    mod device_config {
        include!(concat!(env!("OUT_DIR"), "/handle_table_config.rs"));
    }
    #[cfg(target_pointer_width = "32")]
    pub(super) const SLOTS: usize = device_config::HANDLE_SLOTS;
    #[cfg(not(target_pointer_width = "32"))]
    pub(super) const SLOTS: usize = 1024;

    const INDEX_BITS: u32 = SLOTS.trailing_zeros();
    /// Free-list terminator. `SLOTS` itself, because 0 is a valid slot.
    const EMPTY: u16 = SLOTS as u16;

    // Power of two (index decode is a mask) and small enough that
    // 16 generation bits above the index never reach the i32 sign bit.
    const _: () = assert!(SLOTS.is_power_of_two());
    const _: () = assert!(INDEX_BITS + 16 <= 31);

    // SAFETY/concurrency: same `static mut` + single-threaded-JVM contract
    // as the rest of the LVGL layer (documented in `LvglGfx::init`); tests
    // serialize on a lock.
    static mut PTRS: [*mut lv_obj_t; SLOTS] = [null_mut(); SLOTS];
    static mut GENS: [u16; SLOTS] = [1; SLOTS];
    static mut NEXT: [u16; SLOTS] = initial_free_chain();
    static mut FREE_HEAD: u16 = 0;
    /// Live registrations (diagnostics; reported when the table fills).
    static mut LIVE: u16 = 0;
    /// Handle of the pinned slot (the screen), 0 = none. Survives [`reset`].
    static mut PINNED: i32 = 0;

    const fn initial_free_chain() -> [u16; SLOTS] {
        let mut next = [0u16; SLOTS];
        let mut i = 0;
        while i < SLOTS {
            next[i] = (i + 1) as u16; // last slot links to EMPTY (= SLOTS)
            i += 1;
        }
        next
    }

    #[inline]
    fn encode(gen: u16, idx: usize) -> i32 {
        ((gen as i32) << INDEX_BITS) | idx as i32
    }

    #[inline]
    fn next_gen(g: u16) -> u16 {
        // Skip 0 on wrap so no handle ever encodes to 0 (= Handle::NULL).
        if g == u16::MAX {
            1
        } else {
            g + 1
        }
    }

    /// Register a pointer and return its encoded `nativeHandle`.
    /// Null ptr → 0. Table full → 0 (creation yields `Handle::NULL` and
    /// every subsequent op no-ops) — no device panic.
    pub fn register(ptr: *mut lv_obj_t) -> i32 {
        register_impl(ptr, true)
    }

    /// Register the **screen** object: no `LV_EVENT_DELETE` hook (the active
    /// screen is never deleted — `lifecycle.rs` documents that invariant, and
    /// LVGL does not dedupe hooks, so re-hooking it each boot/reload would
    /// accumulate callbacks) and the slot is pinned so [`reset`] between app
    /// runs preserves the cached `SCREEN_HANDLE`.
    pub fn register_pinned(ptr: *mut lv_obj_t) -> i32 {
        let handle = register_impl(ptr, false);
        unsafe { PINNED = handle };
        handle
    }

    #[inline(never)]
    fn register_impl(ptr: *mut lv_obj_t, install_hook: bool) -> i32 {
        if ptr.is_null() {
            return 0;
        }
        unsafe {
            let head = FREE_HEAD;
            if head == EMPTY {
                note_full();
                return 0;
            }
            let idx = head as usize;
            FREE_HEAD = NEXT[idx];
            PTRS[idx] = ptr;
            LIVE += 1;
            let handle = encode(GENS[idx], idx);
            if install_hook {
                install_delete_hook(ptr, handle);
            }
            handle
        }
    }

    /// Look up the pointer for a handle. Returns null for id ≤ 0 or any id
    /// that doesn't re-encode to the slot's current generation (stale after
    /// delete/reset, or forged/corrupt) — the sanitizer aborts loudly on the
    /// stale case instead when enabled (sim default).
    // inline(never): inlining the decode at the ~99 lookup call sites costs
    // ~7.4 KB of RP2040 flash for a few saved cycles that are noise next to
    // the string-tuple native dispatch each widget call already paid.
    #[inline(never)]
    pub fn lookup(id: i32) -> *mut lv_obj_t {
        if id <= 0 {
            return null_mut();
        }
        let idx = (id as usize) & (SLOTS - 1);
        unsafe {
            if encode(GENS[idx], idx) != id {
                note_stale(id, GENS[idx]);
                return null_mut();
            }
            PTRS[idx]
        }
    }

    /// Invalidate every non-pinned slot and rebuild the free list (called
    /// between app runs). The pinned screen slot survives, so `SCREEN_HANDLE`
    /// stays valid across PDB app reloads.
    pub fn reset() {
        unsafe {
            let pinned_idx = if PINNED > 0 {
                Some((PINNED as usize) & (SLOTS - 1))
            } else {
                None
            };
            FREE_HEAD = EMPTY;
            LIVE = 0;
            // Back-to-front so low slots pop first (keeps handles small).
            let mut i = SLOTS;
            while i > 0 {
                i -= 1;
                if Some(i) == pinned_idx {
                    LIVE += 1;
                    continue;
                }
                if !PTRS[i].is_null() {
                    PTRS[i] = null_mut();
                    GENS[i] = next_gen(GENS[i]);
                }
                NEXT[i] = FREE_HEAD;
                FREE_HEAD = i as u16;
            }
        }
    }

    /// Shared body of the delete hook: invalidate `handle`'s slot iff it
    /// still maps to `obj` (guards against a slot reused since the hook was
    /// installed). Test-callable — the LVGL-integrated path is covered by
    /// the sim suite.
    fn invalidate_if_current(handle: i32, obj: *mut lv_obj_t) {
        if handle <= 0 {
            return;
        }
        let idx = (handle as usize) & (SLOTS - 1);
        unsafe {
            if encode(GENS[idx], idx) == handle && PTRS[idx] == obj {
                PTRS[idx] = null_mut();
                GENS[idx] = next_gen(GENS[idx]);
                NEXT[idx] = FREE_HEAD;
                FREE_HEAD = idx as u16;
                LIVE = LIVE.saturating_sub(1);
            }
        }
    }

    #[cfg(not(test))]
    fn install_delete_hook(ptr: *mut lv_obj_t, handle: i32) {
        // The FULL handle rides in the event user_data, so the hook verifies
        // generation *and* pointer before invalidating — strictly stronger
        // than the old ptr-only guard.
        unsafe {
            crate::lvgl_ffi::lv_obj_add_event_cb(
                ptr,
                Some(handle_delete_cb),
                crate::lvgl_ffi::LV_EVENT_DELETE,
                handle as usize as *mut core::ffi::c_void,
            );
        }
    }

    #[cfg(test)]
    fn install_delete_hook(_ptr: *mut lv_obj_t, _handle: i32) {}

    /// `LV_EVENT_DELETE` hook installed by [`register`]. LVGL fires it for
    /// every descendant during `lv_obj_delete`/`lv_obj_clean`/screen
    /// teardown, so child handles invalidate with their parents.
    #[cfg(not(test))]
    unsafe extern "C" fn handle_delete_cb(e: *mut crate::lvgl_ffi::lv_event_t) {
        let handle = unsafe { crate::lvgl_ffi::lv_event_get_user_data(e) } as usize as i32;
        let obj = unsafe { crate::lvgl_ffi::lv_event_get_target_obj(e) };
        invalidate_if_current(handle, obj);
    }

    // ── Stale/full reporting ────────────────────────────────────────────────

    /// Loud, deterministic stop for a stale handle access. Captures a
    /// backtrace unconditionally — the whole point is to show *where* the
    /// stale lookup came from, regardless of `RUST_BACKTRACE`.
    #[cfg(any(test, feature = "sim"))]
    #[cold]
    #[inline(never)]
    fn report_use_after_delete(id: i32, gen_now: u16) -> ! {
        let backtrace = std::backtrace::Backtrace::force_capture();
        panic!(
            "handle-sanitizer: nativeHandle {id} is stale (its slot's generation is now \
             {gen_now}) — the view was deleted or the table was reset. On the legacy 32-bit \
             hardware path this exact access dangles into freed LVGL memory. Fix the owner to \
             cancel/deregister the handle on delete.\noffending call site:\n{backtrace}"
        );
    }

    #[cold]
    #[inline(never)]
    fn note_stale(id: i32, gen_now: u16) {
        #[cfg(any(test, feature = "sim"))]
        if sanitizer::enabled() {
            report_use_after_delete(id, gen_now);
        }
        #[cfg(not(any(test, feature = "sim")))]
        defmt::warn!("stale handle {=i32} (slot gen now {=u16})", id, gen_now);
    }

    #[cold]
    #[inline(never)]
    fn note_full() {
        #[cfg(any(test, feature = "sim"))]
        {
            if sanitizer::enabled() {
                panic!("handle-sanitizer: LVGL handle table full ({SLOTS} slots) — widget leak?");
            }
            eprintln!(
                "[sim] LVGL handle table full ({SLOTS} slots) — widget creation returns NULL"
            );
        }
        #[cfg(not(any(test, feature = "sim")))]
        defmt::error!(
            "handle table full ({=usize} slots, {=u16} live) — widget creation returns NULL",
            SLOTS,
            unsafe { LIVE }
        );
    }

    /// Whether the stale-handle sanitizer is on: aborts with a backtrace
    /// instead of silently returning null. Read once from
    /// `PICODROID_HANDLE_SANITIZER` (`1`/`on`/`true`/`yes`) and cached —
    /// `scripts/sim.sh` defaults it ON. Tests override via `force`.
    #[cfg(any(test, feature = "sim"))]
    mod sanitizer {
        use core::sync::atomic::{AtomicU8, Ordering};
        // 0 = unread, 1 = off, 2 = on.
        static STATE: AtomicU8 = AtomicU8::new(0);

        pub fn enabled() -> bool {
            match STATE.load(Ordering::Relaxed) {
                1 => false,
                2 => true,
                _ => {
                    let on = std::env::var("PICODROID_HANDLE_SANITIZER")
                        .map(|v| matches!(v.as_str(), "1" | "on" | "true" | "yes"))
                        .unwrap_or(false);
                    STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
                    on
                }
            }
        }

        #[cfg(test)]
        pub fn force(on: bool) {
            STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
        }
    }

    // ── Tests (host-run via the `#[path]` shim in main.rs) ─────────────────
    //
    // Host `cargo test` exercises only the 64-bit pointer width; the 32-bit
    // arm of this same code is compiled + linted by the `handle-table-32`
    // pre-commit legs and behaviorally covered by the HIL soak.
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::Mutex;

        // The table is process-global state; serialize the tests and start
        // each from a clean slate.
        static LOCK: Mutex<()> = Mutex::new(());

        fn setup() -> std::sync::MutexGuard<'static, ()> {
            let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            sanitizer::force(false);
            unsafe { PINNED = 0 };
            reset();
            guard
        }

        fn fake(i: usize) -> *mut lv_obj_t {
            (0x1000 + i * 8) as *mut lv_obj_t
        }

        #[test]
        fn roundtrip_and_null() {
            let _g = setup();
            assert_eq!(register(core::ptr::null_mut()), 0);
            let h = register(fake(1));
            assert!(h > 0);
            assert_eq!(lookup(h), fake(1));
            assert!(lookup(0).is_null());
            assert!(lookup(-3).is_null());
            assert!(lookup(h ^ 0x40_0000).is_null()); // garbage generation bits
        }

        #[test]
        fn forged_high_bits_rejected() {
            let _g = setup();
            let h = register(fake(2));
            // Same low bits, junk above the generation field: must not
            // truncate into a valid (gen, idx) pair (amendment 1).
            let forged = h | 0x4000_0000;
            assert!(forged > 0);
            assert!(lookup(forged).is_null());
            assert_eq!(lookup(h), fake(2));
        }

        #[test]
        fn invalidation_and_slot_reuse_aba() {
            let _g = setup();
            let a = register(fake(3));
            invalidate_if_current(a, fake(3));
            assert!(lookup(a).is_null(), "stale handle must not resolve");
            let b = register(fake(4));
            // Free list hands the same slot back — different generation.
            assert_eq!(a as usize & (SLOTS - 1), b as usize & (SLOTS - 1));
            assert_ne!(a, b);
            assert!(lookup(a).is_null(), "ABA: old handle stays stale");
            assert_eq!(lookup(b), fake(4));
        }

        #[test]
        fn delete_hook_guard_ignores_reused_slot() {
            let _g = setup();
            let a = register(fake(5));
            invalidate_if_current(a, fake(5));
            let b = register(fake(6));
            // A late-firing hook for the OLD registration must not touch the
            // slot's new occupant.
            invalidate_if_current(a, fake(5));
            assert_eq!(lookup(b), fake(6));
        }

        #[test]
        fn generation_wrap_skips_zero() {
            let _g = setup();
            let h = register(fake(7));
            let idx = h as usize & (SLOTS - 1);
            unsafe { GENS[idx] = u16::MAX };
            let h_max = encode(u16::MAX, idx);
            invalidate_if_current(h_max, fake(7));
            unsafe { assert_eq!(GENS[idx], 1, "wrap must skip generation 0") };
            let h2 = register(fake(8));
            assert!(h2 > 0, "post-wrap handle encodes non-zero");
        }

        #[test]
        fn full_table_returns_zero_then_recovers() {
            let _g = setup();
            let handles: Vec<i32> = (0..SLOTS).map(|i| register(fake(i))).collect();
            assert!(handles.iter().all(|&h| h > 0));
            // All SLOTS fresh registers must land in distinct slots
            // (amendment 2 — the free-list chain is actually a chain).
            let mut idxs: Vec<usize> = handles.iter().map(|&h| h as usize & (SLOTS - 1)).collect();
            idxs.sort_unstable();
            idxs.dedup();
            assert_eq!(idxs.len(), SLOTS);
            // Full: no panic, returns 0.
            assert_eq!(register(fake(9999)), 0);
            // One invalidate → register succeeds again.
            invalidate_if_current(handles[0], fake(0));
            assert!(register(fake(10_000)) > 0);
        }

        #[test]
        fn reset_invalidates_everything_but_pinned() {
            let _g = setup();
            let screen = register_pinned(fake(1));
            let w1 = register(fake(2));
            let w2 = register(fake(3));
            reset();
            assert_eq!(lookup(screen), fake(1), "pinned screen survives reset");
            assert!(lookup(w1).is_null());
            assert!(lookup(w2).is_null());
            // Free list fully rebuilt: SLOTS-1 registers succeed (screen
            // still occupies its slot), then the table is full.
            for i in 0..SLOTS - 1 {
                assert!(register(fake(100 + i)) > 0, "register #{i} after reset");
            }
            assert_eq!(register(fake(99_999)), 0);
        }

        #[test]
        fn churn_never_exhausts() {
            let _g = setup();
            // 3× SLOTS sequential register/invalidate cycles — kills the old
            // 4096-cumulative ceiling class of failure.
            for i in 0..SLOTS * 3 {
                let h = register(fake(i));
                assert!(h > 0, "churn iteration {i}");
                assert_eq!(lookup(h), fake(i));
                invalidate_if_current(h, fake(i));
                assert!(lookup(h).is_null());
            }
        }

        #[test]
        #[should_panic(expected = "handle-sanitizer")]
        fn sanitizer_aborts_on_stale_lookup() {
            let _g = setup();
            let h = register(fake(11));
            invalidate_if_current(h, fake(11));
            sanitizer::force(true);
            let _ = lookup(h);
        }
    }
}

pub use imp::{lookup, register, register_pinned, reset};
