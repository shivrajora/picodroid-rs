# Dev log

Newest first. One terse entry per landed change or session: what landed (commit), the
numbers, what is still open. Sessions end abruptly; this file plus a green tree is how the next
one picks up. Routine handovers go here; a large one gets its own doc under `docs/` and a line
here. Keep entries to a few lines; the reasoning goes to [`decisions.md`](decisions.md), the
ranked list to [`gaps.md`](gaps.md).

## 2026-10-22 — the UI-rendering QA's ten findings, fixed

- **Landed:** one commit per finding on `main`, `e556d132..f49c5f90`. `LinearLayout` caps a
  content-sized child with Android's AT_MOST (`max_width` / `max_height` of the cross-axis room,
  so a long label wraps and shrinks back; U1). `SeekBar` reserves its thumb's overhang as padding
  (U2; the CheckBox / RadioButton part was a misread, retracted on a pixel read). `ScrollView`'s
  thumb is the theme's 40 % again (U3). A never-sized `ViewGroup` child takes
  `generateDefaultLayoutParams()`, mirrored from Android per container; `LinearLayout.setOrientation`
  is Java over `nativeSetOrientation`, `getOrientation()` added (U4). `AlertDialog` buttons fit
  their row and the row never scrolls (U5); the card is content-sized and centred by alignment, a
  dialog with no buttons has no row, `board-lib.sh::settings_dialog_ok` derives from the 126 px
  message card (U6). `GridLayout` measures content tracks itself, margins included, on
  `LV_EVENT_CHILD_CHANGED` (U7). `TimePicker` rollers fit the 100 px picker (14 px line space) and
  get `events::groups::key_focus_border` (U8). The matrix says choice rows carry no check mark
  (U9). The fit walk stops at a button and a roller (U10; the layoutdemo half retracted).
- **Also:** `PICODROID_SIM_TREE=1` dumps the live widget tree after the `[layout]` line (sim
  only; it settled both retractions). `pd_lvgl_sys::style_num` reads a numeric style property
  through the cascade (unsafe: picodroid-core 327 → 324, the FFI crate 21 → 22). The wifi
  scan-table test lock's spin carries a `spin-ok` marker (test.sh had failed on it since
  2026-10-20). Rows: `gesturedemo` and `ellipsizedemo` pin `[layout] fit ok`.
- **Verified:** a capture per finding (before/after against the round's), `test.sh` green, and
  the sim rows of the apps touched (listed in the results doc's outcome paragraph).
- **Still open:** nothing from the round. The nightly cron is restored.

## 2026-10-22 — simulator QA: UI rendering across the apps

- **Ran:** 160 captures over 42 apps on four panel geometries, looking for cut text, clipped
  widgets and spacing; the nightly cron was paused for the night (restore: drop the `# PAUSED`
  prefix in `crontab -e`). Results and the fix shapes in
  [`qa-ui-render-2026-10-22-results.md`](qa-ui-render-2026-10-22-results.md); captures under
  `build/qa/ui-render-2026-10-22/shots/`.
- **Found:** five medium (a `wrap_content` TextView cut instead of wrapped, CheckBox / Radio /
  SeekBar drawn past their left edge, the opaque scrollbar track over a scrolling column's last
  glyphs, an unsized nested ViewGroup at 160×160, a three-button dialog's row scrolling under its
  buttons), three low (dialog card height by formula, a GridLayout column short of its margins,
  the TimePicker's focus sliver), two notes. Nothing committed but the doc.
- **Still open:** nothing: every finding closed the same evening (the entry above).

## 2026-10-20 — ListView recycles a window of rows

- **Landed:** `ListView` keeps only the rows in view plus one beyond each edge alive and hands a
  row that scrolls out back to `getView` as `convertView` for the one scrolling in; the list's
  own padding stands in for the rest, a deferred `fireScroll` (new `dispatch_sites` row) slides
  the window, and `focus_order.rs` (pure, 8 host tests) keeps the keypad group in child order
  with `lv_group_swap_obj`. Lists of eight items or fewer are built whole, as before.
  `setSelection`, `smoothScrollToPosition`, `getFirstVisiblePosition` / `getLastVisiblePosition`
  / `getPositionForView`, `ViewGroup.detachViewFromParent` / `attachViewToParent`; two fixes:
  `addView(existingChild, lastIndex)` never moved the widget, and a focused adapter row never
  scrolled into view (only `lv_button` rows carry `SCROLL_ON_FOCUS`), so DOWN past the last
  visible row wrapped the focus; rows now scroll themselves in, since LVGL's own scroll-into-view
  counts the list's padding as an inset. Design:
  `designs/listview-recycling-2026-10.md`.
- **Numbers:** qa_list's 200-item list on a 320×240 panel keeps 10 rows alive (12 built, the
  two over-measured before layout are freed) and re-binds on a drag and on twelve DOWN presses;
  qa_ui's 60-row fixture holds an 8-row window, `setSelection(30)` and a smooth scroll to 59
  land where they should, 282 checks green in both shrink modes. picoenvmon History lists its
  whole 60-sample ring, opened on the newest. Flash: +5,620 B on testbench_rp2040 (+48 B RAM, the scroll ring) and +5,420 B on
  testbench_rp2350, ratchet accepted.
- **Rows:** `qa_list` ×2 (testbench by touch, pico_display2_w by keys; the key presses are
  paced 0.2 s apart because a burst collapses in the button debouncer), `qa_ui`'s
  `listRecycling`. Rows share the first row's height (documented divergence).
- **Merged** to main `ba565e3f` on 2026-10-21; bench-verified the same morning (History rows
  8 of 60 on the W slot, keys walk, dialog).
- **Still open:** a `test.ctrl` for picoenvmon History in the sim (one script must serve a keys
  board and a touch board; the logger yields a sample a second, so a 20 s wait would do);
  `OnScrollListener`, header / footer views, choice modes; sim-run writes both rows of one app
  in one shrink mode to the same log file (pre-existing).

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
- **Native round, tried and dropped (same day):** growing a chain of small objects reached
  `OutOfMemoryError` with 52 KB free — the field arena's copying reallocation asked for
  53 KB — before the native free level ever crossed the 1/16 line, so the native arm cannot
  precede that OOM. Recorded under G3 in `gaps.md` (chunked arenas); `qa_trim` asserts the
  pool half only.
- **CI:** the `Testing` job failed on `4d3cf69f` in two `hal::wifi` scan-table tests that share
  one `static mut` table across parallel test threads (a pre-existing flake; 21/21 locally,
  three runs): they now take a test-local lock.
- **Still open:** chunked arenas (G3); the `onTrimMemory` levels other than
  `RUNNING_CRITICAL` stay constants by decision.

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
