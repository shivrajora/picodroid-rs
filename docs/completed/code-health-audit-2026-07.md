# Completed: Code-Health Audit — 2026-07-24

Items closed out of [code-health-audit-2026-07.md](../code-health-audit-2026-07.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## 3. Axis: test coverage

### 3.4 Gate gaps (compile/lint lanes)

- **`pico_enviro_mon` — the shipping product board — is never cross-compiled to ARM
  firmware by any gate** (pre-commit builds rp2040/rp2350/tdeck; CI builds rp2040/rp2350;
  HIL hardcodes `testbench_rp2350` at `hil-run.sh:28`). Its `sensor-bme688`/`sensor-ltr559`
  hardware paths get no compile coverage anywhere. Known *(roadmap)* but still open.
  **RESOLVED 2026-07-25:** ARM clippy lanes added to the pre-commit board loop and CI
  linting for both this board and `testbench_rp2350w`.
- **`board-testbench-rp2350w` (cyw43 network) is compiled by nothing automated** — feature
  exists in `platforms/rp/Cargo.toml:25`, referenced by zero scripts/CI. It can rot
  silently; decide to gate it or delete it. **RESOLVED 2026-07-25: gated** (kept — Pico 2 W
  is a published board); enabling the gate required adding the missing `# Safety` docs the
  rot had already accumulated in `drivers/cyw43.rs`.

## 4. Axis: modularization

### 4.2 Dead and stale files (six)

| File | State |
|---|---|
| `platforms/rp/src/hal/rp/timer_alarm.rs` (82 ln) | **Orphaned real code — RESOLVED 2026-07-25: deleted.** Archaeology showed `f1c0b0d` (PDB task moved to core 0, 2026-04-12) deliberately retired the whole cross-core park design — it removed the `mod` declaration (and the `signal_park_from_isr` symbol the orphan calls) but forgot the file. Deleted along with the equally-dead `park_for_flash()` and `CORE0_RELEASE`; the six stale doc/comment sites describing the old design were rewritten in the same commit. |

## 7. ESP removal plan (decision 2026-07-24)

**Executed 2026-07-25** — all checklist items below landed (plus stragglers the checklist
missed: the `resolve_board` esp branch in `scripts/lib.sh`, a `monitor_store.rs` doc
comment, `docs/parity-audit.md` scope lines, and the `index.mdx` platform card).

Footprint is small and contained — no git submodule, separate Cargo workspace, nothing in
per-push CI beyond what `test.sh` runs. Checklist for the removal session:

1. **Delete `platforms/esp/`** (own workspace + own `Cargo.lock`; nothing else path-deps
   into it — verified).
2. **`scripts/test.sh`** — remove the `platforms/esp` block (lines ~41-48).
3. **`scripts/pre-commit`** — remove the `tdeck_plus` clippy lane (lines ~122-132); revisit
   the `not(feature="family-rp")` count-4 grep at `:37-42` (the "ESP/no-family path"
   comment) — simplify or re-derive the expected count.
4. **`picodroid-core/Cargo.toml`** — drop `family-esp = []`; also drop the unused
   `freertos-rust` optional dep and fix the stale porting comment (§5).
5. **Docs** — README.md and ARCHITECTURE.md ESP/multi-family sections (fold into the §8
   doc refresh: either delete or mark "dormant — see git history"); website pages
   `get-started/esp32s3.md`, `reference/esp32s3-toolchain.md` (delete + remove sidebar
   entries in `astro.config.mjs`), plus ESP mentions in `build.md`, `index.mdx`,
   `limits.md`, `cargo-aliases.md`, `porting-guide.md`, `architecture.md`,
   `release-notes.md` (edit). Run `npm run build` in `website/` — the links validator will
   catch stragglers.
6. **Leave alone:** `third_party/FreeRTOS-Kernel` Xtensa ports (vanilla upstream
   submodule content); `platforms/rp/build.rs:44`'s `"xtensa"` arm in the target-arch
   match (harmless, generic).
7. **Bonus simplifications unlocked:** `test.sh` drops a whole cargo invocation; the
   duplicated `[patch.crates-io]` littlefs patch in the esp workspace goes away; the
   HAL-contract copy-paste and 17 drifting sim-stub twins (§4/§5 findings) dissolve.

Findings this decision retires: HAL-contract centralization, sim-stub drift, the
concurrency-HAL abstraction (record as future-family cost), and the esp/rp code-sharing
grade. `picodroid-core` stays (rationale in §5).

## 9. Prioritized backlog for fix sessions

> **Progress 2026-07-25 (fix session 1):** P0 items 1-4 all landed (stale twins
> fc896b3, timer_alarm resolution 2bcc858, papk-info fix 7b8330b + tool clippy
> lanes ec47b46, ESP removal 8300bf8, board gates 111234d). P1: item 5
> (papk-format) landed in 4 commits 8ef0326/a643514/016aaea/7bbc8a7; item 7
> (LVGL guards, expanded scope) c62eb3b; item 8 (shrink superset) 6da6a4a;
> item 10 (Java gate) 3e82cdd. Items 6 and 9 have reviewed implementation
> designs in docs/designs/ (47cdcdc) awaiting execution. P2 items all open.
>
> **Progress 2026-07-26 (fix session 2):** **P1 is now closed.** Item 9
> (handle table) landed in a1063ed (view_ops null-guard sweep) + 3d441fb
> (generation-tagged table, 9 unit tests, `handle-table-32` pre-commit
> legs) — the 32-bit device arm stays behind that default-off feature until
> a nightly HIL soak, which is the one remaining step. Prerequisite:
> 040ccb7 freed 16.6 KB of RP2040 flash (crc32fast's 16 KiB table → a
> 64-byte nibble table), as the gate had ~136 B of headroom left. Item 6
> (method cross-check) landed in 009503c: 306 declared triples vs 308 SDK
> `ACC_NATIVE` methods, both directions, mutation-tested; it immediately
> surfaced a live `NoSuchMethod` (`NotificationManager.notify`/`cancel`),
> fixed in the follow-up. Measurements and status are recorded in the two
> design docs and `docs/parity-audit.md` (HAL-05). P2 items open except
> P2-17, delivered as an extraction enabler (see §0).

**P0 — correctness/safety now, all cheap:**

1. Delete the 4 stale core-twins in `platforms/rp/src/` + `examples/androidport`;
   investigate-then-resolve `hal/rp/timer_alarm.rs` (§4.2 — replaced or lost?).
2. Fix `papk-info` `fmt_size`; add clippy lanes for `papk-pack`/`papk-info`/`class-shrink`.
3. Execute the ESP removal checklist (§7).
4. Add a `pico_enviro_mon` ARM firmware-compile gate (pre-commit or CI `building`); decide
   gate-or-delete for `board-testbench-rp2350w`.

**P1 — contract hardening (extend the existing guard pattern):**

5. Extract the `papk-format` crate + round-trip tests (§6.1) — also un-leaks `jvm`.
6. Method-level native-registry cross-check (§6.3, *roadmap* stage 2).
7. Extend the LVGL drift guard to the other constant families (§6.4).
8. Shrink-map superset (vN+1 ⊇ vN) CI test (§6.4).
9. Handle-table invalidation on the 32-bit path + tests on both widths (§6.2).
10. Per-push Java gate: add the remaining self-checking suites (`bytecodecoverage`,
    `prefs_demo`, `clinitdemo`, …) to CI `sim-smoke`, and a fast `sim-run --app langsuite`
    (or similar) lane to pre-commit.

**P2 — structure and polish:**

11. Split `lifecycle.rs` (widget dispatch out via `DISPATCH_SITES`; state machine behind a
    trait) — after the *(roadmap)* sim scenario tests exist as a net.
12. Split `object_heap/mod.rs` (StringBuilder scratch, formatting, exception side-tables
    out of the heap).
14. Make `pdb`'s protocol/papk modules a `[lib]`; wire `parity-bench.sh` into nightly HIL
    or descope it; document the littlefs fork (FORK.md + upstream ref).
17. GC-root provider registry *(roadmap)* — also breaks the `native_handler`↔`graphics`
    cycle (§4.3).

### P2 status audit — 2026-08-31

Verified against the tree, not against this list's own wording:

- **12 — partial.** The StringBuilder scratch *did* come out (`896f691`,
  `jvm/src/object_heap/sb_store.rs`), but `object_heap/mod.rs` is still ~1,516
  lines and still owns the exception side-tables and formatting.
- **14 — the `[lib]` goal was met by a better route, so treat it as done.**
  `tools/pdb/Cargo.toml` still has no `[lib]`, but the shared code was lifted
  into two real workspace crates — `pdb-protocol/` and `papk-format/` — and
  `tools/pdb/src/protocol.rs` is now a thin `pub use pdb_protocol::*` plus std
  I/O. `parity-bench.sh` is wired (pre-commit size lane).
- **17 — DONE** (`23fa075`), as recorded.
- 11 is untouched (`lifecycle.rs` is now 1,983 lines, 0 tests).

### P2 status re-check — 2026-09-16

Against `09e7a8b3`, by looking at the tree:

- **11 — untouched, and bigger.** `crates/picodroid-core/src/lifecycle.rs` is
  2,184 lines with no `#[test]`. Its prerequisite, the sim scenario tests, is
  still a `quality-roadmap.md` entry; the pdb-driven HIL rows
  (`pdb-launch`, `pdb-settings-uninstall`) are the nearest thing to a net.
- **12 — partial, and regressing.** `crates/jvm/src/object_heap/mod.rs` has
  grown to 1,956 lines (from ~1,516); the QA round's formatter and OOM fixes
  landed in it.
- **14 — done.** The littlefs "fork" is now vendored in-tree
  (`third_party/littlefs-rust{,-core}`, with `rp2040-hal` beside it), and the
  root `Cargo.toml` `[patch]` comments name the upstream version and the patch.
  No separate FORK.md; the comments are the record.
- **17 — done** (unchanged).
