// SPDX-License-Identifier: GPL-3.0-only
//! LVGL object handle ↔ Java `nativeHandle` (i32) conversion.
//!
//! One width-independent **generation-tagged table** (audit P1-9 /
//! parity-audit HAL-05): a handle encodes a slot index in its low bits and
//! that slot's generation above them, so a deleted widget's handle goes
//! stale the moment its `LV_EVENT_DELETE` hook fires — [`get`] returns
//! `None` instead of dangling into freed LVGL memory. Slots are reused via a
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
//! **Every target runs this table.** 32-bit devices joined the 64-bit sim on
//! 2026-09-15, once `qa_ui` passed on both RP2350 boards in both shrink modes
//! and the picoenvmon nav + PDB-reload soak ran clean with it on (cost:
//! +2.4-3.0 KB flash, ~1 KB of `.bss`). The legacy zero-cost cast
//! (`ptr as u32 as i32`, no invalidation — the dangle documented as
//! HAL-05/S1) survives one release behind the opt-out `legacy-handle-cast`
//! feature as an escape hatch, and goes away with it. The sim's
//! `PICODROID_HANDLE_SANITIZER` (default-on in `scripts/sim.sh`) turns any
//! stale lookup into a loud abort with a backtrace.

use crate::lvgl_ffi::Obj;

// FFI seam for `cargo test`: the table logic is pure and pointer-width
// independent; only the delete-hook *installation* touches LVGL, and that
// path is exercised end-to-end by the sim suite. Under test the hook is a
// no-op and tests drive invalidation directly.
//
// The table is one of the places an `Obj` comes from (see `Obj`'s docs): a
// slot hands its object out only while the slot is current, and the
// `LV_EVENT_DELETE` hook `register` installs clears the slot before LVGL
// frees the object, so what [`get`] returns names an object not deleted.

// ── Legacy 32-bit cast (no invalidation) — the `legacy-handle-cast` hatch ───

#[cfg(all(target_pointer_width = "32", feature = "legacy-handle-cast"))]
mod imp {
    use super::Obj;

    /// Store a pointer as a Java `nativeHandle`: bit-preserving cast.
    /// A deleted handle *dangles* (see module note) — this arm exists only
    /// as the one-release escape hatch from the generational table.
    #[inline(always)]
    pub fn register(obj: Obj) -> i32 {
        obj.addr() as u32 as i32
    }

    /// Same cast as [`register`] — the screen is not tracked separately.
    #[inline(always)]
    pub fn register_pinned(obj: Obj) -> i32 {
        register(obj)
    }

    /// Recover the object from a Java `nativeHandle`: bit-preserving cast.
    #[inline(always)]
    pub fn get(id: i32) -> Option<Obj> {
        // SAFETY: none that this arm can give. The handle is the pointer, so
        // a live handle names a live object, and a deleted one dangles: the
        // HAL-05 use-after-free the generational table exists to close, which
        // building with `legacy-handle-cast` opts back into.
        unsafe { Obj::from_raw(id as u32 as *mut crate::lvgl_ffi::lv_obj_t) }
    }

    /// Without a table there is nothing to consult: every non-null cast
    /// is taken to be live (a deleted handle dangles, see the module note).
    #[inline(always)]
    pub fn is_live(id: i32) -> bool {
        id != 0
    }

    /// No-op — there is no table to clear.
    #[inline(always)]
    pub fn reset() {}
}

// ── Generation-tagged table: every target, unless `legacy-handle-cast` ──────

#[cfg(any(not(target_pointer_width = "32"), not(feature = "legacy-handle-cast")))]
mod imp {
    use super::Obj;
    use crate::util::local::Core0;
    use core::cell::Cell;

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

    /// The table, slot-indexed. A `None` in `objs[i]` is a free slot. All
    /// zero at boot, so it sits in .bss; the generations and the free chain,
    /// which start non-zero, are [`LINKS`].
    struct Table {
        /// Each object is live while it is here: the delete hook takes it out
        /// before LVGL frees it (`invalidate_if_current`), and a reset drops
        /// every unpinned one.
        objs: [Cell<Option<Obj>>; SLOTS],
        free_head: Cell<u16>,
        /// Live registrations (diagnostics; reported when the table fills).
        live: Cell<u16>,
        /// Handle of the pinned slot (the screen), 0 = none. Survives [`reset`].
        pinned: Cell<i32>,
    }

    struct Links {
        gens: [Cell<u16>; SLOTS],
        /// Free-list links: the slot after `i`, or `EMPTY`.
        next: [Cell<u16>; SLOTS],
    }

    // SAFETY: registered into and looked up by widget natives, invalidated by
    // LVGL's delete callbacks, all on core-0 JVM tasks; the host tests
    // serialize on a lock.
    static TABLE: Core0<Table> = unsafe {
        Core0::new(Table {
            objs: [const { Cell::new(None) }; SLOTS],
            free_head: Cell::new(0),
            live: Cell::new(0),
            pinned: Cell::new(0),
        })
    };

    // SAFETY: `TABLE`'s other half, reached by the same functions only.
    static LINKS: Core0<Links> = unsafe {
        Core0::new(Links {
            gens: [const { Cell::new(1) }; SLOTS],
            next: initial_free_chain(),
        })
    };

    const fn initial_free_chain() -> [Cell<u16>; SLOTS] {
        let mut next = [const { Cell::new(0) }; SLOTS];
        let mut i = 0;
        while i < SLOTS {
            next[i] = Cell::new((i + 1) as u16); // last slot links to EMPTY (= SLOTS)
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

    /// Register an object and return its encoded `nativeHandle`. Table
    /// full → 0 (creation yields `Handle::NULL` and every subsequent op
    /// no-ops) — no device panic.
    pub fn register(obj: Obj) -> i32 {
        register_impl(obj, true)
    }

    /// Register the **screen** object: no `LV_EVENT_DELETE` hook (the active
    /// screen is never deleted — `lifecycle.rs` documents that invariant, and
    /// LVGL does not dedupe hooks, so re-hooking it each boot/reload would
    /// accumulate callbacks) and the slot is pinned so [`reset`] between app
    /// runs preserves the cached `SCREEN_HANDLE`.
    pub fn register_pinned(obj: Obj) -> i32 {
        let handle = register_impl(obj, false);
        TABLE.pinned.set(handle);
        handle
    }

    #[inline(never)]
    fn register_impl(obj: Obj, install_hook: bool) -> i32 {
        let t = &*TABLE;
        let head = t.free_head.get();
        if head == EMPTY {
            note_full();
            return 0;
        }
        let idx = head as usize;
        t.free_head.set(LINKS.next[idx].get());
        t.objs[idx].set(Some(obj));
        t.live.set(t.live.get() + 1);
        let handle = encode(LINKS.gens[idx].get(), idx);
        if install_hook {
            install_delete_hook(obj, handle);
        }
        handle
    }

    /// Whether `id` still names a live widget. Unlike [`get`], a stale
    /// id is an ordinary `false` here, not a use-after-delete for the
    /// sanitizer: this is how `addView` asks before touching a child that
    /// `removeView` may have released.
    pub fn is_live(id: i32) -> bool {
        if id <= 0 {
            return false;
        }
        let idx = (id as usize) & (SLOTS - 1);
        encode(LINKS.gens[idx].get(), idx) == id && TABLE.objs[idx].get().is_some()
    }

    /// Whether `obj` is a registered widget — one some Java object holds
    /// the handle of. A linear scan of the table, so host-only: the D3 check
    /// in `view_ops::delete` asks it once per `View.close()`.
    #[cfg(any(test, feature = "sim"))]
    pub fn is_registered(obj: Obj) -> bool {
        TABLE.objs.iter().any(|p| p.get() == Some(obj))
    }

    /// `View.close()` reached a widget that still sits under a Java-owned
    /// container (claudeusage D3, 2026-09): the container's Java child list
    /// would keep the freed subtree reachable until the container dies — live
    /// heap went 14 KB to 48 KB over a dozen page turns, then
    /// `OutOfMemoryError`. `View.close()` now routes a parented view through
    /// `removeView`, so this means a widget was attached under a Java view
    /// without `addView` recording the parent. Sanitizer on: stop with a
    /// backtrace; off: one warning line.
    #[cfg(feature = "sim")]
    #[cold]
    #[inline(never)]
    pub fn report_close_under_java_parent(id: i32) {
        // The diagnosis first, on its own: under the sim's heap cap the
        // backtrace capture below can fail to allocate (the stale-handle
        // report's known trap), and the run must still name the bug.
        eprintln!(
            "[sim] View.close() on nativeHandle {id} while its widget still sits under a \
             Java-owned container: the parent's child list would keep the freed subtree \
             alive (D3)"
        );
        if !sanitizer::enabled() {
            return;
        }
        // Symbolising a backtrace allocates far more than the capped arena
        // has to spare (192 KB in one block on a 408 KB model): off the
        // model, or the report dies as an OOM abort before it names the bug.
        let _bypass = crate::host::heap_bypass();
        let backtrace = std::backtrace::Backtrace::force_capture();
        panic!(
            "handle-sanitizer: View.close() under a Java-owned container (nativeHandle {id}). \
             View.close() detaches a view whose parent it knows, so whoever attached this \
             widget bypassed ViewGroup.addView; go through addView/removeView.\n\
             offending call site:\n{backtrace}"
        );
    }

    /// The live object a handle names. `None` for id ≤ 0 or any id that
    /// doesn't re-encode to the slot's current generation (stale after
    /// delete/reset, or forged/corrupt) — the sanitizer aborts loudly on the
    /// stale case instead when enabled (sim default).
    // inline(never): inlining the decode at the ~99 call sites costs ~7.4 KB
    // of RP2040 flash for a few saved cycles that are noise next to the
    // native dispatch each widget call already paid.
    #[inline(never)]
    pub fn get(id: i32) -> Option<Obj> {
        if id <= 0 {
            return None;
        }
        let idx = (id as usize) & (SLOTS - 1);
        let gen = LINKS.gens[idx].get();
        if encode(gen, idx) != id {
            note_stale(id, gen);
            return None;
        }
        TABLE.objs[idx].get()
    }

    /// Invalidate every non-pinned slot and rebuild the free list (called
    /// between app runs). The pinned screen slot survives, so `SCREEN_HANDLE`
    /// stays valid across PDB app reloads.
    pub fn reset() {
        let t = &*TABLE;
        let pinned = t.pinned.get();
        let pinned_idx = if pinned > 0 {
            Some((pinned as usize) & (SLOTS - 1))
        } else {
            None
        };
        t.free_head.set(EMPTY);
        t.live.set(0);
        // Back-to-front so low slots pop first (keeps handles small).
        let mut i = SLOTS;
        while i > 0 {
            i -= 1;
            if Some(i) == pinned_idx {
                t.live.set(t.live.get() + 1);
                continue;
            }
            if t.objs[i].replace(None).is_some() {
                LINKS.gens[i].set(next_gen(LINKS.gens[i].get()));
            }
            LINKS.next[i].set(t.free_head.get());
            t.free_head.set(i as u16);
        }
    }

    /// Shared body of the delete hook: invalidate `handle`'s slot iff it
    /// still maps to `obj` (guards against a slot reused since the hook was
    /// installed). Test-callable — the LVGL-integrated path is covered by
    /// the sim suite.
    fn invalidate_if_current(handle: i32, obj: Obj) {
        if handle <= 0 {
            return;
        }
        let idx = (handle as usize) & (SLOTS - 1);
        let t = &*TABLE;
        if encode(LINKS.gens[idx].get(), idx) == handle && t.objs[idx].get() == Some(obj) {
            t.objs[idx].set(None);
            LINKS.gens[idx].set(next_gen(LINKS.gens[idx].get()));
            LINKS.next[idx].set(t.free_head.get());
            t.free_head.set(idx as u16);
            t.live.set(t.live.get().saturating_sub(1));
        }
    }

    #[cfg(not(test))]
    fn install_delete_hook(obj: Obj, handle: i32) {
        // The FULL handle rides in the event user_data, so the hook verifies
        // generation *and* pointer before invalidating — strictly stronger
        // than the old ptr-only guard.
        // SAFETY: `handle_delete_cb` reads its user data back as the integer
        // passed here, never as a pointer.
        unsafe {
            crate::lvgl_ffi::lv_obj_add_event_cb(
                obj,
                Some(handle_delete_cb),
                crate::lvgl_ffi::LV_EVENT_DELETE,
                handle as usize as *mut core::ffi::c_void,
            );
        }
    }

    #[cfg(test)]
    fn install_delete_hook(_obj: Obj, _handle: i32) {}

    #[cfg(not(test))]
    crate::lvgl_ffi::lv_event_cb! {
        /// `LV_EVENT_DELETE` hook installed by [`register`]. LVGL fires it
        /// for every descendant during `lv_obj_delete`/`lv_obj_clean`/screen
        /// teardown, so child handles invalidate with their parents.
        fn handle_delete_cb(e) {
            invalidate_if_current(e.user_data() as i32, e.target());
        }
    }

    // ── Stale/full reporting ────────────────────────────────────────────────

    /// Loud, deterministic stop for a stale handle access. Captures a
    /// backtrace unconditionally — the whole point is to show *where* the
    /// stale lookup came from, regardless of `RUST_BACKTRACE`.
    #[cfg(any(test, feature = "sim"))]
    #[cold]
    #[inline(never)]
    fn report_use_after_delete(id: i32, gen_now: u16) -> ! {
        // Off the simulated heap, as in `report_close_under_java_parent`:
        // the capture is host work and must not be what runs out.
        #[cfg(feature = "sim")]
        let _bypass = crate::host::heap_bypass();
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
            TABLE.live.get()
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
    // arm of this same code is what every board build compiles, and is
    // behaviorally covered by the qa_ui HIL rows and the picoenvmon soak.
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
            TABLE.pinned.set(0);
            reset();
            guard
        }

        fn fake(i: usize) -> Obj {
            // SAFETY: the tests only store and compare these; nothing
            // dereferences them.
            unsafe { Obj::from_raw((0x1000 + i * 8) as *mut crate::lvgl_ffi::lv_obj_t) }.unwrap()
        }

        #[test]
        fn is_registered_follows_the_table() {
            let _g = setup();
            assert!(!is_registered(fake(7)));
            let h = register(fake(7));
            assert!(is_registered(fake(7)));
            assert!(!is_registered(fake(8)));
            invalidate_if_current(h, fake(7));
            assert!(!is_registered(fake(7)));
        }

        #[test]
        fn roundtrip_and_null() {
            let _g = setup();
            let h = register(fake(1));
            assert!(h > 0);
            assert_eq!(get(h), Some(fake(1)));
            assert!(get(0).is_none());
            assert!(get(-3).is_none());
            assert!(get(h ^ 0x40_0000).is_none()); // garbage generation bits
        }

        // J-fix a97321ee: `is_live` is how `addView` asks whether a child
        // `removeView` may have released is still there. Unlike `get`, a
        // stale id is an ordinary `false` -- not a use-after-delete the
        // sanitizer aborts on -- so the question can be asked safely.
        #[test]
        fn is_live_answers_for_stale_and_forged_handles_without_faulting() {
            let _g = setup();
            sanitizer::force(true);
            assert!(!is_live(0), "the null handle is never live");
            assert!(!is_live(-1), "a negative handle is never live");
            let h = register(fake(7));
            assert!(is_live(h));
            // Asking about a released widget is a plain `false`.
            invalidate_if_current(h, fake(7));
            assert!(!is_live(h), "a released handle must not read as live");
            // And the slot's next occupant does not revive the old handle.
            let next = register(fake(8));
            assert!(is_live(next));
            assert!(!is_live(h), "ABA: the old handle stays stale");
            // Forged generation bits are not live either.
            assert!(!is_live(next | 0x4000_0000));
        }

        #[test]
        fn forged_high_bits_rejected() {
            let _g = setup();
            let h = register(fake(2));
            // Same low bits, junk above the generation field: must not
            // truncate into a valid (gen, idx) pair (amendment 1).
            let forged = h | 0x4000_0000;
            assert!(forged > 0);
            assert!(get(forged).is_none());
            assert_eq!(get(h), Some(fake(2)));
        }

        #[test]
        fn invalidation_and_slot_reuse_aba() {
            let _g = setup();
            let a = register(fake(3));
            invalidate_if_current(a, fake(3));
            assert!(get(a).is_none(), "stale handle must not resolve");
            let b = register(fake(4));
            // Free list hands the same slot back — different generation.
            assert_eq!(a as usize & (SLOTS - 1), b as usize & (SLOTS - 1));
            assert_ne!(a, b);
            assert!(get(a).is_none(), "ABA: old handle stays stale");
            assert_eq!(get(b), Some(fake(4)));
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
            assert_eq!(get(b), Some(fake(6)));
        }

        #[test]
        fn generation_wrap_skips_zero() {
            let _g = setup();
            let h = register(fake(7));
            let idx = h as usize & (SLOTS - 1);
            LINKS.gens[idx].set(u16::MAX);
            let h_max = encode(u16::MAX, idx);
            invalidate_if_current(h_max, fake(7));
            assert_eq!(LINKS.gens[idx].get(), 1, "wrap must skip generation 0");
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
            assert_eq!(get(screen), Some(fake(1)), "pinned screen survives reset");
            assert!(get(w1).is_none());
            assert!(get(w2).is_none());
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
                assert_eq!(get(h), Some(fake(i)));
                invalidate_if_current(h, fake(i));
                assert!(get(h).is_none());
            }
        }

        #[test]
        #[should_panic(expected = "handle-sanitizer")]
        fn sanitizer_aborts_on_stale_lookup() {
            let _g = setup();
            let h = register(fake(11));
            invalidate_if_current(h, fake(11));
            sanitizer::force(true);
            let _ = get(h);
        }
    }
}

pub use imp::{get, is_live, register, register_pinned, reset};
#[cfg(feature = "sim")]
pub use imp::{is_registered, report_close_under_java_parent};
