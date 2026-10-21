# Dev log

Newest first. One terse entry per landed change or session: what landed (commit), the
numbers, what is still open. Sessions end abruptly; this file plus a green tree is how the next
one picks up. Routine handovers go here; a large one gets its own doc under `docs/` and a line
here. Keep entries to a few lines; the reasoning goes to [`decisions.md`](decisions.md), the
ranked list to [`gaps.md`](gaps.md).

## 2026-10-20 — gap-review housekeeping

- **Docs only, from the 2026-10-16 gap review's drift list:** `android-parity-roadmap-2026-08.md`
  corrected in place (T1.9 done, `getString(int, Object...)` and styles landed, the
  `ScheduledExecutorService` row) with a dated amendment; the weather and askclaude READMEs and
  Javadoc no longer claim an `SntpClient` anchor the F5 time service replaced on 2026-10-09;
  picoclock's `Clock.floorDiv`/`floorMod` delegate to `Math` instead of claiming it lacks them.
- **Still open:** nothing from this round; the code-side drift (picoenvmon and the system apps
  predate `res/`, `NetworkCallback` and `getLaunchIntentForPackage`) stays under gaps G11.

## 2026-10-20 — the per-class RAM rule withdrawn; `onTrimMemory`

- **Finding:** the gap analysis's "each touched class costs 3 KB of RAM" (and the class-store
  item it pointed at) was stale since class-link (`f5d93245`, 2026-09-28): a registered class
  costs 16 B, touched or not. `claudeusage`'s `TimeFormat` is back to
  `LocalDateTime.ofInstant(..)`; the mem-diag census before and after is the same line
  (350 classes, `tableB~=5612`, `nused` 177,872 B). G11 of the claudeusage roadmap closed; the
  roadmap entry now carries only the flash half, sized by `framework_class_file_framing`
  (`class_metadata_tests.rs`): 132 KB of class-file framing in the host corpus, 22–25 % of the
  section, a G4 lever (row 9b), not built.
- **Landed:** `ComponentCallbacks` / `ComponentCallbacks2` (`onTrimMemory`, `onLowMemory`, the
  `TRIM_MEMORY_*` constants), `Context.registerComponentCallbacks` / `unregisterComponentCallbacks`;
  `Activity`, `Application` and `Service` implement them, with a Java-side registry in `Context`
  and one dispatch site (`Context.fireTrimMemory`). `TRIM_MEMORY_RUNNING_CRITICAL` is delivered
  from the event loop once per episode (`memory_pressure.rs` arms: native heap below 1/16 free
  through the new O(1) `native_heap_free_bytes` hook every turn, the LVGL pool below 1/8 every
  8th frame) and at an Activity launch before a covered Activity is reclaimed; `onLowMemory` when
  one still has to go. `Service.performDestroy` trampoline (so a base-class `onDestroy` is found).
  Sim verb `memtrim <level>`. `qa_trim` row (sim, `rp2350,rp2350b` on the bench). Decisions in
  `decisions.md`; matrix, `api/ui.md`, `api/services.md`, parity tsv updated.
- **Numbers:** flash +4,872 B on `testbench_rp2040` (1,084,672 → 1,089,544) and +5,144 B on
  `testbench_rp2350` (1,559,804 → 1,564,948), static RAM ±0, ratchet accepted; `test.sh` both
  lanes, `pre-commit`, the helloworld smoke, and the `reclaimdemo`, `qa_life` and `qa_trim`
  rows in both shrink modes green in the sim. Not yet run on the bench.
- **Still open:** a native-heap phase for `qa_trim` (a Java cache dropped returns nothing to the
  native heap, so only the pool half is asserted); a bench run of the `qa_trim` row; the
  `onTrimMemory` levels other than `RUNNING_CRITICAL` stay constants by decision.

## 2026-10-20 — the agent harness round

- **Landed (uncommitted at the time of writing):** `CLAUDE.md` rewritten as the orientation
  guide (map, golden rules, picking work, definition of done, parallel agents), `AGENTS.md`
  symlinked to it, nested `CLAUDE.md` in seven areas, `.claude/rules/{code-style,java-sdk,testing,bench}.md`
  promoted from one machine's session memory into the repo.
- **New guards:** the panic ratchet (`check-source-guards.sh --panics`,
  `scripts/panic-baseline.conf`: 99 sites across eight crates) with
  `#![deny(clippy::unwrap_used, ...)]` on the ten crates already at zero and a `clippy_leaf`
  leg in pre-commit and CI; crate layering (`scripts/layers.conf`, `check-layers.py`, L0–L5 +
  host + test); `scripts/scorecard.py` generating `docs/scorecard.md` (`--check` in pre-commit
  and CI) with `sdk/android-parity.tsv` as the first measured Android-parity number: 54.5 %
  of 229 listed classes, floor 54.0.
- **New docs:** `docs/gaps.md` (G1–G16, ranked), this log, `docs/decisions.md`,
  `docs/designs/agent-driving-2026-10.md` (not built: `pdb dump-ui`, `expect-ui`, the driving page).
- **Still open:** commit and push when asked; G2 (agent-driving) is roadmap only by decision;
  the promoted memory files are deleted from the memory directory, the rest stay.

## 2026-10-19 — fuzz ring 2, java.lang.Thread, layouts and the everyday members

- Fuzz ring 2 (`79054425`, merged `4a5b242f`): eleven new targets (26 in all), the leaf crates
  `pd-codec` / `pd-protobuf` / `pd-pdb`, `RxBuffer` and `scan_region` seams; fixed
  `inflate_scan::frame_offset` overflow and a chunked-body payload loss. Flash −812 B rp2040,
  −2,672 B rp2350. `fuzz.sh` names the host triple on every call (`82f9c405`: CI's prebuilt
  cargo-fuzz is musl).
- `picodroid.concurrent.Thread` is `java.lang.Thread` (`791716b1`);
  `Thread.UncaughtExceptionHandler` compile-only and pre-commit runs the embed guards
  (`1f571977`, 2026-10-20).
- The everyday-members round (`2092249e..86a26130`): `getContext` / `post` / `onTouchEvent`,
  custom `ViewGroup` + `onLayout`, `isNaN`, `StringBuilder` editing, `java.util.function`,
  `onKey` with keyCode, `Lifecycle` enums + observers, `MediatorLiveData`, `java.io` stream
  types, `EditText extends TextView`. `layoutdemo` gained a custom `FlowLayout` (`d784e90d`).
- Layouts (`6f549b71`, 15 commits): `GridLayout`, `RelativeLayout`, the `LinearLayout` solver
  with per-child gravity and Android weights; ratchet +33 KB accepted. Release v0.39.0
  (`ed03b460`).
- **Open:** `Link::from_raw` under Miri; a week of nightlies on the new targets.

## 2026-10-18 — broadcasts

- Android broadcasts (`305f0dfd`): `BroadcastReceiver`, `IntentFilter`, `sendBroadcast`, the
  system actions as one bitmask upcall per frame, `PendingIntent.getBroadcast`, manifest
  `<receiver>` via headless runs; the three apps that polled the clock now receive
  `ACTION_TIME_TICK`. `check-shrunk-image.sh` learned prefixes and adjacent pending names
  (`09af785b`, `47bb7096`). Release v0.38.0 (`e02bae5d`).

## 2026-10-17 — tree-owned listeners and the build budget

- View listeners are fields of the view, bound through the handle table; the 32-per-kind native
  tables and the `legacy-handle-cast` hatch are gone; `AlertDialog` / `Snackbar` held while
  shown. The 250 ms screen-build budget replaced the 50 ms line; one-native inflate
  (6.1 → 2.3 ms per view on the RP2350); Settings, Launcher, picoclock and claudeusage build in
  `onCreate` (`b2c5f39a..9940d41d`). HIL and 34 sim rows green.

## 2026-10-16 — dependency bumps

- FreeRTOS+TCP V4.4.2, kernel V11.3.2, minifb 0.29; `strcasecmp` shim for the W boards;
  ratchet accepted (`2e451394`, +2,652 B rp2040 / +2,940 B rp2350 from the refreshed lock).
  Blocked majors recorded in `decisions.md`.

## 2026-10-15 — fuzzing, nightly variants, coverage, the unsafe review

- 15 cargo-fuzz targets + stable replay (`92de9913`); a GC-stress pass and a strict mem-diag
  soak in `sim-run.sh` (`8a8aaf92..6d8860d3`), which found a push-rooting gap and a
  thread-terminate ABA; coverage tooling (`39ecb15b`: `coverage.sh` unit + `--sim`, a
  report-only CI job); unsafe reduction round 2 (`f7652f0d`: 980 → 517 blocks, 0 undocumented);
  UR-1..15 fixed and verified (`7a083084`, `cad6ab70`), with the C shim byte store and the
  `test.sh` compile break found on the way.
- **Open:** the coverage ratchet is not gating; the `spidemo` hw row; rejoin on a dead bus.

## 2026-10-14 — flash structure and the device serial

- Line numbers off on device (−71.5 KB on the W debug image), LVGL stubs (−6.8 KB), a
  `board:debug` trend row (`0d21d0af..2b61d750`); the float-formatting rewrite cost +3.2 KB and
  was reverted. Per-device serial in USB, the PING greeting and `Build.getSerial()`; `pdb`
  picks devices adb-style (`e5d84a9f..daed5224`), fixing the `pdb-install` HIL flake.

## 2026-10-12 to 13 — flash and dispatch

- HIL fixes 1–4 (`5c35278e`: early exit, touchless launch SKIP, no per-row core rebuild);
  the section Utf8 pool (`6c205169`, −42 KB rp2040 section); hash-keyed native dispatch
  (`3b654785`, memcmp 868 → 267, −9.3 / −9.8 KB); the duplicate-code campaign (14 commits,
  −31,964 B rp2040 / −42,472 B rp2350).

## 2026-10-11 — bug scan follow-ups

- A1–A4 + C10c pushed (`2ab9d250..14cedd99`); nightly before / after rp2040 28 → 0 failures.
  `pico_display2_w` debug link overflowed in CI: `app_region_kb` 1280 → 1024 (`1112ddbf`).
  Bench handoff: `bench-handoff-2026-10-11.md`.
