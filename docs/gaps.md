# Where Picodroid falls short

> **Last reviewed:** 2026-10-20 against `1f571977` · **Change:** first edition, seeded from `docs/README.md` "Open work", `quality-roadmap.md`, the `*follow-ups*.md` files and `docs/scorecard.md`.

Every known shortfall, one entry each, ranked by how much it stands between an Android
developer and a working app on a board. **This is the work list agents pick from:** take the
highest-ranked entry nobody is working on (say so in `docs/devlog.md`), and when you close part
of one, update its evidence and estimate here and the doc it points at, in the same commit.
Measured numbers come from [`scorecard.md`](scorecard.md) (regenerate it, never type a number
here that it can produce). The follow-up files stay as the evidence; this page is the only one
that ranks.

Hours are agent-hours (one Fable / Opus-class agent working sequentially, tests and docs
included), first-edition estimates to be recalibrated as rounds land. **Human** marks what an
agent cannot finish alone: a decision, a bench action, a key, a push to someone else's repo.

## Summary

| # | Gap | Kind | Impact | Estimate | Evidence |
|---|---|---|---|---:|---|
| G1 | Android API parity: the missing and shallow classes | API surface | high | 120–200 h + human | [scorecard](scorecard.md#android-api-parity), [compatibility matrix](../website/src/content/docs/reference/compatibility-matrix.md) |
| G2 | Agents cannot inspect a running UI | tooling | high | 12–20 h | [designs/agent-driving-2026-10.md](designs/agent-driving-2026-10.md) |
| G3 | 99 panic sites left in eight firmware crates | reliability | high | 10–20 h | [scorecard](scorecard.md#reliability), `scripts/panic-baseline.conf` |
| G4 | Flash headroom on the largest images | size | high | 40–80 h + human | [designs/flash-budget-2026-09.md](designs/flash-budget-2026-09.md) §8 |
| G5 | Test depth: coverage floors, the gating ratchet, the Java layer | testing | medium-high | 40–80 h | [coverage.md](coverage.md), [designs/java-sdk-coverage-2026-10.md](designs/java-sdk-coverage-2026-10.md) |
| G6 | Networking and TLS follow-ups | networking | medium | 30–60 h + human | [networking-followups-2026-08.md](networking-followups-2026-08.md), [tls-follow-ups.md](tls-follow-ups.md) |
| G7 | Packages: signing, network install, the store, permissions | platform | medium | 60–120 h + human | [designs/app-store-roadmap-2026-09.md](designs/app-store-roadmap-2026-09.md) |
| G8 | Simulator memory model overcharges the RP2350 | parity | medium | 30–60 h | [parity-audit.md](parity-audit.md) "2026-09-24 memory-model divergence" |
| G9 | Scheduling audit residue | scheduling | medium | 20–40 h + hardware | [scheduling-audit-2026-09.md](scheduling-audit-2026-09.md) |
| G10 | Fragments: child managers, animations, a bridge-backed row | API surface | medium | 20–40 h | [fragments-follow-ups.md](fragments-follow-ups.md) |
| G11 | The showcase apps' open decisions and performance rows | apps | medium | 30–60 h + human | [designs/claudeusage-decisions-2026-10.md](designs/claudeusage-decisions-2026-10.md), [designs/picoclock-roadmap.md](designs/picoclock-roadmap.md) |
| G12 | Nightly hygiene: a clean worktree, region gates, HIL coverage | infrastructure | medium | 10–20 h | [quality-roadmap.md](quality-roadmap.md) |
| G13 | PSRAM and scroll: the stages left on the touch boards | graphics | low-medium | 30–60 h + hardware | [designs/psram-lvgl-fluid-scroll-2026-09.md](designs/psram-lvgl-fluid-scroll-2026-09.md), [designs/scroll-performance-2026-09.md](designs/scroll-performance-2026-09.md) |
| G14 | Code health: engine crates, executors, facade tests, READMEs | structure | low-medium | 40–80 h | [code-health-2026-09.md](code-health-2026-09.md), [code-health-audit-2026-07.md](code-health-audit-2026-07.md) |
| G15 | Unsafe-review residue and the open Miri finding | soundness | low | 6–12 h + hardware | [bugs-unsafe-review-2026-10.md](bugs-unsafe-review-2026-10.md), [designs/fuzzing-2026-10.md](designs/fuzzing-2026-10.md) |
| G16 | New frontiers: Meshtastic, a language model on the MCU | roadmap | low | 200–400 h each + human | [designs/meshtastic-roadmap-2026-08.md](designs/meshtastic-roadmap-2026-08.md), [designs/edge-llm-roadmap-2026-08.md](designs/edge-llm-roadmap-2026-08.md) |

## The gaps

### G1. Android API parity: the missing and shallow classes

- **Missing:** the scorecard's in-scope `missing` rows: `RecyclerView` (ListView builds every row, no scrap pool), `ConstraintLayout`, a plain `Dialog`, `ValueAnimator` / `ObjectAnimator` / `Animation`, `TextClock`, `Chronometer`, `TableLayout`, `Window`, `Path` / `Rect` / `RectF` / `Matrix` / `Typeface`, `ClipboardManager`, `Vibrator`, `ConcurrentHashMap`, `ReentrantLock`. And the depth behind the `partial` rows: implicit `Intent` resolution, `Parcelable`-free extras beyond two ints, `Canvas` without `Path` or clipping, `AlertDialog.setView`, `EditText` multi-line, `MotionEvent` with one pointer.
- **Evidence:** `scorecard.md` (54.5 % on 2026-10-20: 38 live, 152 partial, 19 missing, 20 out of scope); every port in `docs/designs/claudeusage-android-shape-2026-09.md` and the QA rounds found the same classes.
- **Impact:** this is the project goal. A developer looks for these by name.
- **Estimate:** 120–200 h; each class 2–12 h with its flash cost measured on `testbench_rp2040`. **Human:** which out-of-scope rows (Bitmap, Handler) stay out, and the flash budget each new SDK class may spend (G4).

### G2. Agents cannot inspect a running UI

- **Missing:** a way to read the live view tree (class, id, bounds, text, focus) from the sim and a board, so a UI claim can be asserted without a human at the screen; one page that says how to launch, drive and capture an app.
- **Evidence:** `sim-shot.sh` gives pixels and a `[layout]` line only; `pdb input` drives a device blind; the recipes are spread over four pages. `designs/agent-driving-2026-10.md` has the design.
- **Impact:** every UI change today is verified by eye or by a log line; photocraft's `ui.inspect` is what let its agents test what they built.
- **Estimate:** 12–20 h (`pdb dump-ui`, a `sim-ctrl.sh` verb, `expect-ui` in `test.ctrl`, the page).

### G3. 99 panic sites left in eight firmware crates

- **Missing:** `#![deny(clippy::unwrap_used, ...)]` on `picodroid-core` (40 sites), `platforms/rp` (34), `pdb-protocol` (11), `pd-tls` (5), `class-link` (4), `jvm` (2), `papk-format` (2), `pd-install` (1). Ten crates deny them already.
- **Evidence:** `scripts/panic-baseline.conf`; eight JVM allocations that reset the board under a full heap were found by the fuzz round (fixed 2026-10-15).
- **Also missing:** the JVM's arenas (`ObjectHeap::fields_arena`, `ArrayHeap::arena`) grow by a copying reallocation — exact-sized, but the new block is taken while the old one is held — so a growing arena needs twice its size contiguous and an app sees `OutOfMemoryError` with half the heap free (`qa_trim`'s native round, 2026-10-20: a 53,248 B request refused with 52,224 B free). Chunked arenas, as `ChunkedSlots` already does for the slot stores, would remove it; until then the native-heap `onTrimMemory` arm cannot precede that OOM, since the steady free level never crosses its line. About 8–16 h.
- **Impact:** a panic on a board is a reset with the app's state gone.
- **Estimate:** 10–20 h; each site is a `?`, an `unwrap_or*` or a comment proving infallibility. Lower the baseline per crate, then add the deny attribute and the crate to `stage_clippy_leaf`.

### G4. Flash headroom on the largest images

- **Missing:** the `pico_display2_w` debug image was 2,121,664 of 2,621,440 B on 2026-10-14 with about 190 KB of growth per fortnight; the open levers of `flash-budget-2026-09.md` §8: row 4 (LVGL config), 5 (float formatting; the rewrite cost +3.2 KB and was reverted), 6 (SDK tree-shake and `LV_USE_*`), 7 (`no-pdb`), 9 (a shared string table: done as the Utf8 pool, 2026-10-13) and 9b (the class-file framing the link table has made redundant: measured 2026-10-20 at 22–25 % of the framework section, roughly 105–115 KB on `testbench_rp2350`, a layout 4 / PAPK v4 change of 30–50 h; `quality-roadmap.md` §"A class store of our own"). A region-percentage gate (quality roadmap, Stage 2) so the wall is a CI failure with a margin, not a link error.
- **Evidence:** `scorecard.md` firmware size; `project/release-notes.md`; the 2026-10-11 cut of `app_region_kb` 1280 → 1024.
- **Impact:** every SDK class in G1 is embedded on every board; without headroom G1 stalls.
- **Estimate:** 40–80 h. **Human:** which features may leave a board (`pdb`, LVGL widgets) and whether the RP2040 testbench keeps every SDK class.

### G5. Test depth

- **Missing:** `picodroid-core` at 44.3 % line coverage, `pd-tls` 52.5 %, `pd-rtos` 64.3 %, `build_support` 31.7 %, `papk-info` 28.0 %; the coverage ratchet is committed but not gating; the Java SDK coverage layer is a design only; a lifecycle / store unit-test layer, the Kotlin contention test and the screenshot and sensor injection the sim lacks (quality roadmap).
- **Evidence:** `scripts/coverage-baseline.conf`, `coverage.md`, `designs/java-sdk-coverage-2026-10.md` ("design only, not built").
- **Impact:** the native handlers and the lifecycle are where the QA rounds found their bugs; they are the least covered.
- **Estimate:** 40–80 h.

### G6. Networking and TLS follow-ups

- **Missing:** NET-1 (BCME -5 iovars), NET-8 (WPA3 built, unverified against a real WPA3 AP), NET-3 (the upstream cyw43 bsscfg PR, prepared), NET-9 (socket I/O chunk size); TLS-4 (flash levers), TLS-5 (a live `askclaude` run), TLS-6 (`pdb prefs set` key provisioning), TLS-7 (the handshake task's stack); mDNS (G7 of the claudeusage roadmap).
- **Evidence:** the two follow-up files; NET-12 (one join in thirty never completed) was fixed 2026-10-10 with 437 clean power cycles.
- **Impact:** the WiFi boards are the showcase; each item is a failure a user would see.
- **Estimate:** 30–60 h. **Human:** a WPA3 access point on the bench; pushing the upstream PR.

### G7. Packages: signing, network install, the store, permissions

- **Missing:** S3 signing, S4 network install, S8 the store app, S9 permissions; S2 and S6 partial (streaming `Session`, store app).
- **Evidence:** `designs/app-store-roadmap-2026-09.md`; the installer, the app region and `pdb install` over USB and the sim socket are built and fuzzed.
- **Impact:** an app reaches a board only over USB today.
- **Estimate:** 60–120 h. **Human:** key management and what a permission means on a board with no user accounts.

### G8. Simulator memory model overcharges the RP2350

- **Missing:** the sim charges 79 KB more than the board on the same screen (pointer-width tables: parsed class metadata, the class table, statics, dispatch memos); M8 pointer-free tables, M9 the network stack's boot budget, M10 the LVGL pool at the device's 48-byte object size.
- **Evidence:** `parity-audit.md` "2026-09-24 memory-model divergence"; the heap attribution ledger (`memory-diagnostics.md`).
- **Impact:** "sim OOM but device fine" costs a bench session each time; the rule is that every divergence is a simulator bug.
- **Estimate:** 30–60 h; M8 also shrinks the device's RAM.

### G9. Scheduling audit residue

- **Missing:** WP7's timer-reprogramming half (waits on an event-driven indev), WP9 (UART TX ring + IRQ), the G6 `sched-diag` HIL rows, WP0's one verification pass (button and GT911 wakes), the W-slot `blinky pdb launch` stall.
- **Evidence:** the status table at the top of `scheduling-audit-2026-09.md`; the handover doc.
- **Estimate:** 20–40 h; the verification pass needs the bench.

### G10. Fragments

- **Missing:** FR-9 (child fragment managers, animations, `setRetainInstance`, menus), FR-10 (a bridge-backed nightly row that turns real pages), FR-11 (keys on button-less boards).
- **Evidence:** `fragments-follow-ups.md`; FR-4, FR-8, FR-12 closed.
- **Estimate:** 20–40 h.

### G11. The showcase apps' open decisions and performance rows

- **Missing:** claudeusage decisions F3 and F7 (deferred), D4's runtime residual, G5 alpha assets, G6 backlight PWM, H3 (drop `java.time`, a flash lever only since 2026-10-20), H10 (the JVM task stack), P1–P5 (page-turn performance beyond SRAM placement); picoclock R3–R15 (R12 partial).
- **Evidence:** `designs/claudeusage-decisions-2026-10.md`, `designs/claudeusage-gaps-roadmap-2026-09.md`, `designs/picoclock-roadmap.md`.
- **Estimate:** 30–60 h. **Human:** F3 and F7.

### G12. Nightly hygiene

- **Missing:** the nightly runs the live working tree (uncommitted edits included; the SHA is a label): pin it to a clean worktree or stamp `DIRTY`; a region-percentage size gate (Stage 2); the coverage ratchet gating; HIL board / app coverage beyond the fleet defaults.
- **Evidence:** `quality-roadmap.md` "Pin nightly runs to a clean worktree", "Binary-size regression report" Stage 2.
- **Estimate:** 10–20 h.

### G13. PSRAM and scroll on the touch boards

- **Missing:** PSRAM Stage 5 (class bytes in PSRAM), §7 step 9 (a pre-rendered page), a PSRAM tenant that pays; scroll S2 (measure on the touch kit), S6b (entry paint, DatePicker), S7 (tearing / TE pin), S8 (GP11).
- **Evidence:** the two design docs' status lines; the LVGL pool measured in PSRAM and left off (read `psram-lvgl-fluid-scroll` §4.1 before flipping it).
- **Estimate:** 30–60 h; needs the touch kit and the Plus 2 W on the bench.

### G14. Code health

- **Missing:** `pd-hal` / LVGL engine crates, the back-stack transitions, the executors extraction, file splits, jvm facade tests and `invoke_*` consolidation (audit item 13), crate READMEs (15), script hardening (16).
- **Evidence:** `code-health-2026-09.md` backlog; `code-health-audit-2026-07.md` §9.
- **Estimate:** 40–80 h; each item is its own commit with the ratchets as the gate.

### G15. Unsafe-review residue and the open Miri finding

- **Missing:** the two items the 2026-10-15 verification left (the `spidemo` hw row, rejoin on a dead bus) and `Link::from_raw`, which fails Stacked Borrows under Miri.
- **Evidence:** `bugs-unsafe-review-2026-10.md` (read its status line: the fixes landed 2026-10-15), `designs/fuzzing-2026-10.md` amendments.
- **Estimate:** 6–12 h; the hw row needs the bench.

### G16. New frontiers

- **Missing:** a board and app on-air compatible with Meshtastic (nine sessions planned); a language model on the MCU across three tracks (track B adds a `platforms/stm32` family). Neither started; most of edge-LLM's A1 exists as `pico_touch_kit`.
- **Estimate:** 200–400 h each. **Human:** hardware, and whether either is wanted before G1–G5.
