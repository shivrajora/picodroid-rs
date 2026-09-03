# Design: Generation-tagged widget handle invalidation (audit P1-9)

> Produced 2026-07-25 by the audit fix session's design panel (4 parallel
> design agents, each adversarially critiqued against source; the critique's
> verdict and amendments are at the bottom and OVERRIDE the design body where
> they conflict). Execute from this doc; update it if reality diverges.

## DESIGN
# Generation-Tagged Widget Handle Table — Design (audit §6.2 / parity-audit HAL-05, S1)

## 0. Current state (verified in source)

- `platforms/rp/src/system/picodroid/graphics/lvgl/handle_table.rs` (190 ln, zero tests):
  - **32-bit (device)**: `register = ptr as u32 as i32`, `lookup = id as u32 as *mut lv_obj_t`, `reset()` no-op. No invalidation of any kind; a deleted handle dangles into freed LVGL memory (caused the HW-only animation hang, patched around via `animations::cancel_subtree` + `keyboard::unbind_if_deleting` in `view_ops.rs:118-173`).
  - **64-bit (sim)**: `HANDLES: [*mut lv_obj_t; 4096]` + monotonic `COUNT` (no slot reuse — hard `assert!` panic at 4096 *cumulative* widgets), per-object `LV_EVENT_DELETE` hook stamping a `DELETED` sentinel, `lookup` → null on stale (or abort with backtrace when `PICODROID_HANDLE_SANITIZER` is on — **already default-ON in `scripts/sim.sh:22`**, opt-out `--no-sanitize-handles`; X3 from the parity audit is done).
- `Handle` newtype (`graphics/gfx/handle.rs`) is already opaque: `0 = NULL`, negative→NULL at the Java boundary (`from_java`), so the i32 bit layout is a backend-private detail we are free to redefine. Java sees only `View.nativeHandle` (`sdk/java/picodroid/view/View.java:30`), stored and passed back verbatim — Java never interprets the bits.
- **No reverse ptr→handle path exists**: all LVGL-callback→Java resolution goes through `PtrMap`s keyed on the *raw pointer* (`events.rs` VIEW_KEY/TOUCH/SWIPE/FOCUS maps → `obj_ref: u16` Java heap refs), and `view_ops::child_at` explicitly returns `Handle::NULL` because there is no reverse map. This is what makes the migration tractable.
- `socket_table.rs` (32 slots, has `remove()`) and `net/http_table.rs` repeat the same cast-on-32-bit pattern — same disease, lower stakes (native-owned lifetimes with explicit close).
- Device RAM: rp2040.toml `ram_kb=256, heap_kb=128` (→128 KB for .bss/.data/stacks); rp2350.toml `ram_kb=520, heap_kb=416` (→104 KB static, reported ~98% consumed, i.e. low-single-digit KB of headroom).

## 1. Recommended scheme: one width-independent generation table

Replace both cfg arms with a **single implementation** compiled on both widths (pointer entries are 4 B on device, 8 B on host; everything else identical). Delete the `DELETED` sentinel and the monotonic-COUNT design.

### 1.1 Encoding

```
nativeHandle (i32, always > 0 when valid):
  bits 0..8   = slot index      (INDEX_BITS = 8 → 256 slots)
  bits 8..24  = generation      (u16, full width stored per slot)
  bits 24..31 = 0               (sign bit never set → i32 always positive,
                                 Handle::from_java's `<=0 → NULL` keeps working)
```

- Generations start at 1 and skip 0 on wrap (`0xFFFF → 1`), so an encoded handle is never 0 → `Handle::NULL == 0` remains unambiguous with **all 256 slots usable** (no reserved slot 0).
- ABA window: a stale handle only false-validates if the *same slot* is reused exactly 65,536 times while the stale Java reference is still live. Effectively sound for this system (u8 generations would give 1/256 — not worth the 512 B saved).

### 1.2 Storage (module statics, same `static mut` + `&raw mut` idiom as the rest of the LVGL layer; single-threaded JVM contract, as documented in `LvglGfx::init`)

```rust
const SLOTS: usize = 256;                       // device; see §2 sizing
static mut PTRS: [*mut lv_obj_t; SLOTS] = [null_mut(); SLOTS];
static mut GENS: [u16; SLOTS]           = [1; SLOTS];
static mut FREE_HEAD: u16               = 0;    // intrusive free list:
                                                // a free slot's PTRS[i] stores
                                                // the next free index (as usize),
                                                // so the free list costs 0 extra RAM
static mut LIVE: u16                    = 0;    // for pdb sysmon / diagnostics
```

### 1.3 API (signatures unchanged for all 30+ call sites)

```rust
pub fn register(ptr: *mut lv_obj_t) -> i32
// null ptr → 0. Pop free list; PTRS[i] = ptr; handle = ((GENS[i] as i32) << 8) | i.
// Install the LV_EVENT_DELETE hook (as the 64-bit path does today) with the FULL
// handle as user_data (not just the slot id) — the hook then verifies BOTH
// generation and PTRS[i] == target before invalidating, strictly stronger than
// today's ptr-only guard in handle_delete_cb.
// Table full → defmt::error + return 0 (device); sanitizer abort (sim). No panic
// on device: creation returns Handle::NULL and every subsequent op no-ops.

pub fn lookup(id: i32) -> *mut lv_obj_t
// id <= 0 → null. idx = id & 0xFF; gen = (id >> 8) as u16;
// GENS[idx] != gen → stale: sanitizer abort (std) / defmt::warn (device) / null.
// else PTRS[idx].

pub fn invalidate_slot(...)   // internal, called by the delete hook:
// PTRS[i] = <free-list link>; GENS[i] = wrapping_add(1) skipping 0; push free list.

pub fn reset()
// Invalidate every live slot (bump gen, rebuild free list). See §4.3 for the
// screen-handle interaction — reset() alone is NOT sufficient on device.
```

The delete hook is the load-bearing invalidation path and it is **already HW-proven infrastructure**: a dozen widgets and all four listener maps already install `LV_EVENT_DELETE` callbacks on device (`events.rs`, `check_box.rs`, `switch.rs`, …), and LVGL fires them for every descendant during `lv_obj_delete`/`lv_obj_clean`/screen teardown — exactly the coverage the 64-bit table relies on today.

## 2. (a) Table sizing and RAM cost

| Config | PTRS | GENS | free list | total .bss |
|---|---|---|---|---|
| Device, 256 slots | 1024 B | 512 B | 0 (intrusive) + 4 B heads | **~1.55 KB** |
| Device, 128 slots (fallback) | 512 B | 256 B | ~4 B | **~0.78 KB** |
| Host, 256 slots | 2048 B | 512 B | 0 | ~2.5 KB (down from 32 KB today) |

- 256 *concurrent* slots is generous: with reuse the ceiling is live widgets, not cumulative (the 4096-cumulative panic disappears — `graphicsbench`'s churn becomes a non-issue). Empirically the LVGL heap itself (~50–100 B/obj + styles) cannot host many hundreds of live widgets, and the project has a documented ~12-focusable-row renderer cap on list length.
- RP2350 at 98% static: 1.55 KB must fit in the remaining budget. If it does not, (i) drop to 128 slots via a board-tunable const (precedent: the `prereserve_*` board tunables), or (ii) note that the FreeRTOS heap arena (`heap_kb`) is the tunable pressure valve — shaving 2 KB off `heap_kb=416` in `mcus/rp/rp2350.toml` is a one-line trade if .bss truly cannot absorb it. RP2040 (128 KB static budget) absorbs 1.55 KB trivially.
- Hidden second cost, must be stated: each `register` now adds one LVGL event callback on device (today only the 64-bit path does). LVGL 9 event descriptors cost ~12–16 B of *LVGL heap* per object → ~1 KB at 64 live widgets, ~4 KB worst-case at 256. This is heap the device path never paid before.

## 3. (b) Hot-path cost

`lookup` = shift/mask decode (~2 insn) + bounds check (statically true for idx<256 after `& 0xFF`, so just the `id<=0` test) + one u16 load + compare/branch + one word load.

- Cortex-M33 @150 MHz (RP2350): ~8–12 cycles ≈ 60–80 ns.
- Cortex-M0+ @133 MHz (RP2040): ~15–20 cycles ≈ 150 ns.

Every widget native call already traverses: JVM interpreter dispatch → `native_handler` **string-tuple match** over (class, method) → the LVGL call itself → render invalidation. That pipeline is hundreds–thousands of cycles; the lookup adds well under 1%. The heaviest lookup consumer is `animations::tick` (≤16 slots × 60 Hz ≈ 1k lookups/s ≈ 10k cycles/s — noise). The zero-cost cast is not worth preserving; **acceptable**.

## 4. (c) Stale-lookup policy per call-site class

| Call-site class | Behavior on stale | Rationale |
|---|---|---|
| `view_ops` property setters/getters (`set_pos`, `frame`, …) | **null → early-return no-op** (+ device `defmt::warn!("stale handle {idx} gen {got}!={cur}")`) | Android-faithful: mutating a destroyed/detached View is a silent no-op. **Required sweep**: `set_pos/set_size/set_bg_color/set_padding/set_visibility/set_enabled/set_alpha/set_flex_grow/frame` currently pass `obj(h)` to LVGL *unguarded* — this is a latent null-deref on the 64-bit path **today** whenever the sanitizer is off (`frame()` calls `lv_obj_update_layout(null)`). Fix regardless of this design. |
| `animations::apply` / `cancel_subtree` | silent null (already handled; `apply` has the null check, `cancel_subtree` runs **before** `lv_obj_delete` so generations are still valid — ordering preserved) | per-frame path; a warn every frame would spam |
| Listener registration (`events.rs register_view_*`, widget trampoline installers) | null → early return (already guarded) + warn | registering on a dead view is an app-logic smell worth one log line |
| `delete` / `remove_child` / `remove_all_children` | silent no-op (already guarded — "already deleted") | double-delete is a normal teardown race |
| Sim (std) any class | `PICODROID_HANDLE_SANITIZER` (default ON) → abort with backtrace, now catching stale lookups on the **shared** code path, not a sim-only shadow implementation | |
| Device | `defmt::warn` (not `debug_assert` — debug firmware builds disable debug-assertions for the RP2040 flash gate, so it would be dead there anyway) | permanent-log precedent: the `native miss` defmt log |

## 5. (d) Migration — call sites that assume handle==ptr roundtrip

Audit result: **no site converts a raw `lv_obj_t*` back into a Java handle by cast.** All raw-pointer→Java flows go PtrMap→`obj_ref`. Concrete migration list:

1. **`handle_table.rs`** — rewrite as the unified module; keep the legacy 32-bit cast pair verbatim under `#[cfg(all(target_pointer_width = "32", not(feature = "handle-table-32")))]` during staging (§8).
2. **`view_ops.rs`** — null-guard sweep listed in §4 (independent bug fix; land first as its own commit).
3. **`app.rs:221` reset path + `lifecycle.rs:67` SCREEN_HANDLE** — the critical behavioral change: LVGL init is latched **once per boot** (`LvglGfx::init` INITIALIZED flag) and `SCREEN_HANDLE` is registered once, but `app.rs` calls `handle_table::reset()` between app runs (PDB reload). Today that's a device no-op, so the screen handle survives; a real table that clears everything would kill it on the *second* app load — a PDB-reload-only, HW-only regression. Fix: after `reset()`, `app.rs` calls a new `lvgl::lifecycle::reregister_screen()` that re-registers `lv_screen_active()` and refreshes `SCREEN_HANDLE`. (This also fixes the same latent bug in today's 64-bit table — sim just never reloads apps in-process, so it's invisible.)
4. **`keyboard.rs` SYSTEM_KEYBOARD_HANDLE** — verify `reset_keyboard_state()` (called in the same `app.rs` block) nulls it; the keyboard is lazily re-created, so slot reuse is fine.
5. **`animations.rs`** — no code change (stores i32 handles, resolves via `lookup`); `cancel_subtree` **stays** — it prevents one frame of animating a dying subtree and is the belt to the table's suspenders.
6. **Widgets + events.rs registration sites (~30 `register`, ~90 `lookup`)** — zero signature changes; recompile only.
7. **`socket_table.rs` / `http_table.rs`** — out of scope here; once the widget table soaks, extract a generic `generational_registry<const N>` and adopt (they additionally need their explicit `remove()` mapped to `invalidate`). *2026-09-02:* the no-op-`remove()` leak this warned about is closed — `e8a05ef` gave both tables real slot-reusing `remove()` (now under `picodroid-core/src/net/`); the shared generic registry extraction is still open.

## 6. (e) Sanitizer fold-in

The sanitizer stops being a 64-bit-only shadow and becomes a *mode of the shared table*: stale-lookup detection is now structural (generation mismatch) on both widths; the sanitizer only decides the *response* (abort+backtrace vs warn+null). Keep the env-gated `sanitizer_enabled()` under `#[cfg(feature = "sim")]`/std; it is already default-ON in `sim.sh` and CI, so every sim run exercises the exact invalidation logic the device ships. Generation mismatch also lets the sanitizer distinguish "stale" (`current_gen > encoded`) from "forged/corrupt" (`encoded > current`) in its message. Device gets the always-on `defmt::warn` tier instead.

## 7. (f) Test plan

**Unit tests (in `handle_table.rs` `#[cfg(test)]`, run by `./scripts/test.sh` on host — the same width-independent code the device compiles, only pointer width differs):**
- register→lookup roundtrip; null ptr → 0; id≤0 / garbage id → null.
- delete-hook invalidation → lookup null (drive `invalidate` directly; the LVGL-integrated path is covered by sim).
- **Slot reuse**: register A, invalidate, register B → B gets same index, different generation; A's old handle → null, B's handle → B's ptr (the ABA case the bitset alternative fails).
- **Generation wrap**: force `GENS[i]=0xFFFF`, invalidate → gen becomes 1 (skips 0), handle never encodes to 0.
- Table full → returns 0, no panic; after one invalidate, register succeeds again.
- `reset()` → all pre-reset handles null; free list fully rebuilt (register SLOTS times succeeds).
- Sanitizer: make the enabled-check injectable for tests (env vars are process-global under `cargo test`); `#[should_panic]` on stale lookup with it forced on.
- Churn regression: 3× SLOTS sequential register/invalidate cycles never panics (kills the 4096-cumulative ceiling).

**Sim integration:** the standard smoke set (`helloworld`, `benchmark`, `blinky`) plus `graphicsbench` (widget churn) with sanitizer default-ON now exercises the shared table end-to-end including real `LV_EVENT_DELETE` cascades (screen switches, `lv_obj_clean`). Add a picoenvmon nav-soak sim-remote script pass (History screen + Live-screen back-out — the two historical UAF reproducers).

**HIL (before flipping the default):** nightly `hil-run` on pico_enviro_mon: 4-button nav soak + PDB `input` injection churn + **PDB app reload** (the §5.3 screen-handle case, which sim structurally cannot cover), watching RTT for the new stale-handle `defmt::warn` and for `native miss`.

## 8. (g) Staging

Yes — and staging is mandatory given S1 + no HIL in-session:

1. **Commit 1 (no flag):** `view_ops` null-guard sweep — pure hardening, fixes a live sim bug.
2. **Commit 2 (no flag needed for sim):** unified table **replaces the 64-bit implementation outright** — the sim suite + default-on sanitizer + new unit tests immediately validate the shared logic. The device keeps the cast: `#[cfg]` selects cast unless cargo feature `handle-table-32` (platforms/rp, default off) is set. Includes the `reregister_screen()` plumbing (harmless in cast mode).
3. **Soak:** nightly sim-run green; one `hil-run --app picoenvmon` with `handle-table-32` on, plus PDB reload test.
4. **Commit 3:** flip `handle-table-32` into default features; keep the cast path one release as the escape hatch; then delete it and the feature.

## 9. Alternative considered — validity bitset keyed by ptr — REJECTED

Keep the cast, add a bitset of live `lv_obj_t*` (bit = `(ptr - lvgl_heap_base) / 8`; ~768 B for a 48 KB LVGL heap; set on register, cleared by the same delete hook, lookup checks the bit).

- **Fatal: no ABA protection.** When LVGL's allocator reuses a freed block for a *new* widget at the same address (the common case in a churning UI), the bit is re-set and a stale handle to the *old* widget validates against the *new* one → silent wrong-widget operations. That is precisely the aliasing failure the 64-bit module comment says slot-reuse-without-generations would cause, and it is *worse* than today's crash/hang because it corrupts UI state silently.
- Couples the table to LVGL allocator internals (heap base/bounds, 8-byte granularity, `lv_obj_t` being the allocation head — not guaranteed across the vendored LVGL bump).
- Saves only ~0.8 KB vs the 256-slot table while still paying the identical per-widget delete-hook cost, and gives the sim/device *different* semantics again — reopening the exact "sim-invisible bug class" HAL-05 exists to close.

The generation table costs ~0.75 KB more, is allocator-agnostic, gives identity (not just liveness), unifies both widths onto one tested code path, and removes the sim's 4096-cumulative panic. **Recommendation: the unified generation-tagged table, 256 slots (128-slot board tunable as RP2350 fallback), staged behind `handle-table-32`.**

## KEY FILES
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/lvgl/handle_table.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/lvgl/view_ops.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/lvgl/animations.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/lvgl/events.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/lvgl/lifecycle.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/lvgl/mod.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/gfx/handle.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/graphics/lvgl/widgets/keyboard.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/app.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/net/socket_table.rs
/home/shiv/projects/picodroid-rs/platforms/rp/src/system/picodroid/net/http_table.rs
/home/shiv/projects/picodroid-rs/platforms/rp/mcus/rp/rp2350.toml
/home/shiv/projects/picodroid-rs/platforms/rp/mcus/rp/rp2040.toml
/home/shiv/projects/picodroid-rs/scripts/sim.sh
/home/shiv/projects/picodroid-rs/scripts/test.sh
/home/shiv/projects/picodroid-rs/docs/parity-audit.md
/home/shiv/projects/picodroid-rs/docs/code-health-audit-2026-07.md

## RISKS
RP2350 static RAM at ~98%: the ~1.55 KB .bss table may not fit without dropping to 128 slots (board tunable) or shaving heap_kb; must be measured with an actual link of the board-pico-enviro-mon build before commit 2.
New per-widget LVGL-heap cost on device: register() now installs an LV_EVENT_DELETE callback per widget (~12-16 B LVGL heap each) that the 32-bit path never paid; on the ~30 KB-short picoenvmon heap budget this could tip an OOM — measure with --mem-diag.
PDB app-reload regression: LVGL init is latched once per boot but app.rs calls handle_table::reset() between runs, so a real table invalidates SCREEN_HANDLE on the second load — HW-only, sim-invisible (sim never reloads in-process); requires the reregister_screen() fix and an explicit HIL reload test.
No HIL coverage in this session and the bug class is documented sim-invisible: the 32-bit switch must stay behind the default-off feature until a nightly hil-run + PDB reload soak passes on real hardware.
Behavioral change on device: code that today 'works' by dereferencing freed-but-not-yet-reused LVGL memory will start silently no-oping; correct, but may surface as subtle UI differences only on hardware.
view_ops null-guard sweep is load-bearing: with the table, stale handles return null far more often, and today set_pos/set_size/frame/etc. pass unguarded pointers into LVGL — missing one call site converts a dangle into a null-deref (also a latent 64-bit sim bug right now with the sanitizer off).
Generation ABA is bounded, not zero: a stale Java handle held across 65,536 reuses of one slot false-validates (u16 generations); accepted as negligible.
socket_table.rs and http_table.rs keep the unsafe cast pattern until the later generic-registry follow-up — this design does not close those two instances.

## SCOPE
Roughly one focused week in three staged commits: (1) view_ops null-guard sweep ~100 LOC, low risk, lands immediately; (2) unified generation table replacing the 64-bit path + feature-gated 32-bit switch + reregister_screen plumbing + unit tests, ~450-600 LOC across handle_table.rs/lifecycle.rs/app.rs (call sites need recompile only — signatures unchanged); (3) default flip after a nightly sim-run plus one HIL soak (picoenvmon nav + PDB reload), one-line feature change plus cast-path deletion a release later. Sizing must be validated with a real RP2350 link and a --mem-diag heap check before commit 2 merges.

## EXECUTION LOG (2026-07-26)

Commits 1–2 landed (`a1063ed` view_ops null-guard sweep, `3d441fb` unified
generation-tagged table). All seven amendments applied: re-encode-equality
decode, separate `NEXT` free-list array (`EMPTY = SLOTS`), screen pinned via
new `register_pinned()` (no delete hook, survives `reset()` — no
`reregister_screen()`), `handle-table-32` clippy legs for thumbv6m+thumbv8m
plus an rp2040 feature-on build (flash gate) in pre-commit, slot counts
decoupled 256 device (board-tunable `handle_slots`) / 1024 host, 9 unit
tests incl. full-then-recover + all-slots-distinct, and the measurements
below. **Commit 3 (default flip) is pending the nightly HIL soak.**

Measurements:

- `graphicsbench` peak **43 live** widgets (sim, instrumented); picoenvmon
  hub ~6 — 256 device slots are generous.
- Prerequisite discovered: the RP2040 debug flash gate had only ~136 B of
  headroom (the d14919c-era ~40 KB was gone). Freed 16,652 B by replacing
  `crc32fast`'s 16 KiB table with a 64-byte nibble-table CRC32 (`040ccb7`).
- RP2040 (helloworld, debug) with `handle-table-32`: **+2,688 B text,
  +1,024 B bss**. `lookup` is `inline(never)` — inlining it at ~99 call
  sites cost 7.4 KB of flash.
- RP2350 `pico_enviro_mon` (picoenvmon): **+1,024 B bss** feature-on
  (506,472 → 507,496), well inside the ~26 KB static headroom measured at
  baseline — the 128-slot fallback is not needed.
- Per-widget delete-hook LVGL-heap cost (device-only new cost): ~12–16 B ×
  ≤43 live ≈ **0.7 KB** of the 48 KB `lv_mem` arena by arithmetic; confirm
  on hardware during the HIL soak (sim already paid this cost on the old
  64-bit table, so `--mem-diag` in sim shows no delta by construction).
- Sim validation (sanitizer on): graphicsbench PASSED; 4-cycle picoenvmon
  nav soak through History/Live/Settings — zero sanitizer aborts.

## CRITIQUE VERDICT: needs_changes

### ISSUES
- Decode truncation false-validation: lookup's `gen = (id >> 8) as u16` drops bits 24-31, so a corrupt/forged positive id (e.g. 0x01000102) truncates to a valid (gen, idx) pair and can alias a live slot — strictly weaker than claimed, and it voids the sec.6 'stale vs forged' diagnostic (which is also wrong after generation wrap, since current_gen > encoded doesn't order a wrapped counter).
- Intrusive free list is broken as specified: FREE_HEAD=0 with PTRS=[null;N] is not a valid initial chain — first pop reads PTRS[0]==null==0, so slot 0 is handed out repeatedly (immediate handle aliasing). The design never specifies chain construction or an empty-list sentinel, and 0 is a valid slot index so it cannot double as the sentinel.
- reregister_screen() accumulates LVGL delete hooks: register() installs an LV_EVENT_DELETE callback each call and LVGL does not dedupe (events.rs:721-723 documents this exact hazard); the screen object is never deleted (lifecycle.rs: 'we never call lv_screen_load'), so each PDB reload permanently grows its callback list and LVGL heap — an unbounded leak precisely in the long-soak/PDB-reload scenario the fix targets.
- ~~CI cfg-matrix hole in staging~~ **— closed.** `scripts/pre-commit` now runs clippy for *both* boards with `--features "$BOARD_FEATURE,handle-table-32"` and gates the rp2040 build with `PICODROID_EXTRA_FEATURES=handle-table-32`, so the unified table's 32-bit/no_std arm is compiled and linted on every commit rather than first at flip time. Original risk text: with `handle-table-32` default-off, pre-commit clippy (RP2040+RP2350) and the embedded build never compile that arm between commit 2 and the commit-3 default flip; no_std violations (std leakage in the sanitizer path, provenance casts, defmt format types) would surface only at flip time.
- Host table sizing vs graphicsbench: the current 64-bit module comment says graphicsbench 'deliberately churns hundreds of widgets'; if its concurrently-live count can approach 256, the default-on sanitizer aborts the sim smoke on table-full. The design fixes SLOTS=256 on both widths without measuring graphicsbench's live-widget ceiling.
- Unit tests only run at 64-bit pointer width (scripts/test.sh forces the host target for the whole workspace), so the width-sensitive pieces (integer-in-pointer free-list storage in a 4-byte PTRS array) are never executed at 32 bits by any test; only the clippy/build leg and HIL cover them.
- Risk list omits RP2040 flash: the new table code plus always-on defmt::warn strings add roughly a kilobyte against the documented ~896K ceiling (current headroom ~40 KB per project history — acceptable, but it should be measured and stated alongside the RP2350 RAM check).
- Cosmetic but trap-laden: widget slot fields literally named `handle` (snackbar.rs:39, toast.rs:31, alert_dialog.rs, time_picker.rs) store raw lv_obj_t pointers as usize, not table handles — the design's 'no ptr->handle roundtrip' audit conclusion is correct (verified), but the migration commit should not rename or touch these, and socket_table.rs's 32-bit remove() is already a literal no-op (a slightly worse baseline than the design describes).

### AMENDMENTS
1) Harden lookup decoding: reject any id whose bits 24-31 are nonzero before decoding (or decode idx/gen and require `((gens[idx] as i32) << 8) | idx == id`); drop or rewrite the sec.6 stale-vs-forged claim — after u16 wrap, generation ordering is meaningless, so report only "generation mismatch" plus both values. 2) Specify the free list concretely: either a separate `NEXT: [u16; SLOTS]` array (+512 B device — still under 2.1 KB total, simplest and testable) or a const-built intrusive chain via `core::ptr::without_provenance_mut` (const-stable), and define an explicit empty sentinel (`SLOTS as u16`, since 0 is a valid slot); add a unit test that 256 fresh registers yield 256 distinct slots. 3) Pin the screen slot instead of re-registering: give handle_table a `reset_except(handle)` (or a dedicated pinned slot for the screen) so app.rs reset() preserves SCREEN_HANDLE across PDB reloads, and skip LV_EVENT_DELETE hook installation for the screen registration entirely (it is never deleted per lifecycle.rs's documented invariant) — this removes both the reload regression and the duplicate-hook accumulation; keep reregister_screen() only as a fallback if pinning is rejected, in which case it must lv_obj_remove_event_cb the prior hook. 4) In commit 2, add a `handle-table-32`-enabled clippy + cargo-build leg for both thumbv6m and thumbv8m to pre-commit (precedent: the board-pico-enviro-mon clippy leg) so the device arm of the unified table is compiled and linted from day one; use that same link to do the promised RP2350 .bss measurement and record the RP2040 flash delta. 5) Decouple slot counts: SLOTS=256 device / 1024 host (host cost ~10 KB, still 3x smaller than today's 32 KB), after measuring graphicsbench's peak live-widget count under `--app graphicsbench` with the sanitizer on; wire the device count through the existing board-tunable const mechanism as already proposed. 6) Add to the test plan: a table-full-then-recover test at the device SLOTS constant, and note explicitly that host `cargo test` exercises only 64-bit pointer width so the 32-bit arm's coverage comes from the new clippy/build leg plus the HIL soak. 7) Add the RP2040 flash delta to the risks list, and note in sec.5.7 that socket_table's 32-bit remove() is currently a no-op (its later generational adoption fixes a real leak, not just a cast). Everything else verified against source and stands: view_ops unguarded-setter sweep (set_pos/set_size/set_bg_color/set_padding/set_visibility/set_enabled/set_alpha/set_flex_grow/frame confirmed unguarded; delete/remove/set_parent/child_count confirmed guarded), sim.sh sanitizer default-ON, app.rs:221/lifecycle.rs:67/mod.rs INITIALIZED latch interaction, keyboard.rs:371 nulling, PtrMap raw-pointer keying, no handle==ptr roundtrip, toml RAM figures, and the delete-hook precedent across ~17 widget files.
