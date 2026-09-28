# Engineering docs

The working record of *why* Picodroid is built the way it is: design docs, audits, roadmaps
and dated investigations. Written by and for the people — and agents — doing the work.

**This is not the user manual.** Nothing here is published, nothing here is kept evergreen,
and a doc dated 2026-07 describes 2026-07. Read the status line before you trust a page.

## Where documentation lives

| Tree | Audience | Holds |
|---|---|---|
| [`docs/`](.) (here) | contributors, agents | designs, audits, roadmaps, bug records. Dated, append-only, superseded rather than rewritten. |
| [`website/src/content/docs/`](../website/src/content/docs/) | app developers | the published manual at <https://shivrajora.github.io/picodroid-rs/> — API reference, guides, porting guide, known issues. Kept current; edits go through the site build. |
| repo root | everyone | [`README.md`](../README.md) (what Picodroid is), [`ARCHITECTURE.md`](../ARCHITECTURE.md) (how the runtime fits together), [`CONTRIBUTING.md`](../CONTRIBUTING.md) (how to work on it), [`CLAUDE.md`](../CLAUDE.md) (the rules every session must follow). |

If a fact belongs in an app developer's hands, it goes on the website. If it is the reasoning
behind a decision, or a measurement, or a list of what is still broken, it goes here.

## How to read a doc here

- **The status line under the title is the truth about the doc.** `landed`, `open`,
  `not started`, `superseded`. If it disagrees with the index below, the doc wins — and
  fix the index.
- **Amendments at the bottom override the body.** Several designs were executed from the doc
  while reality diverged; the amendment section records the divergence and is authoritative.
  `freertos-host-sim.md` is the extreme case: read its amendments *first*.
- **Numbers are dated and tied to a commit.** A doc that says "measured against `86ebdfe`"
  means exactly that. Re-measure before you rely on a number from a different tree.
- **A design doc is not a plan to re-execute.** Most of the designs below are already built.
  They are here to explain the shape of the code you are about to change.

## Start here, by what you are about to touch

| Working on | Read, in this order |
|---|---|
| The JVM heap, GC, threading | [designs/jvm-run-lock-2026-09.md](designs/jvm-run-lock-2026-09.md), [memory-diagnostics.md](memory-diagnostics.md), [parity-audit.md](parity-audit.md), [followups-2026-08.md](followups-2026-08.md) |
| Scheduling, a busy-wait, a new delay | [scheduling-audit-2026-09.md](scheduling-audit-2026-09.md) (status table at the top), [scheduling-diagnostics.md](scheduling-diagnostics.md), [scheduling-audit-handover-2026-09.md](scheduling-audit-handover-2026-09.md) |
| LVGL, drawing, scrolling, touch | [designs/sram-hotpath-2026-09.md](designs/sram-hotpath-2026-09.md), [designs/scroll-performance-2026-09.md](designs/scroll-performance-2026-09.md), [designs/band-height-120-2026-09.md](designs/band-height-120-2026-09.md), [designs/psram-lvgl-fluid-scroll-2026-09.md](designs/psram-lvgl-fluid-scroll-2026-09.md), [designs/handle-table-invalidation.md](designs/handle-table-invalidation.md), [designs/rgb565-swapped-render-2026-09.md](designs/rgb565-swapped-render-2026-09.md), [designs/canvas-2026-09.md](designs/canvas-2026-09.md) |
| Networking | [designs/tls-2026-09.md](designs/tls-2026-09.md), [designs/network-seam-2026-09.md](designs/network-seam-2026-09.md), [designs/wifi-provisioning-2026-09.md](designs/wifi-provisioning-2026-09.md), [designs/net-typed-exceptions.md](designs/net-typed-exceptions.md), [networking-followups-2026-08.md](networking-followups-2026-08.md), [tls-follow-ups.md](tls-follow-ups.md), [designs/cyw43-pio-transport.md](designs/cyw43-pio-transport.md) |
| A new board, or a new MCU family | [designs/porting-seam-2026-09.md](designs/porting-seam-2026-09.md), [designs/shared-core-extraction.md](designs/shared-core-extraction.md), [designs/family-neutral-residue.md](designs/family-neutral-residue.md), and the published [porting guide](../website/src/content/docs/reference/porting-guide.md) |
| Flash or RAM budget, the shrinker | [designs/flash-budget-2026-09.md](designs/flash-budget-2026-09.md), [designs/unconditional-shrink-2026-09.md](designs/unconditional-shrink-2026-09.md), [designs/value-slot-8b.md](designs/value-slot-8b.md) |
| Packages, install, the launcher | [designs/multi-app-2026-09.md](designs/multi-app-2026-09.md), [designs/app-store-roadmap-2026-09.md](designs/app-store-roadmap-2026-09.md), [designs/alarm-manager-2026-09.md](designs/alarm-manager-2026-09.md) |
| The Java API surface | [designs/android-parity-roadmap-2026-08.md](designs/android-parity-roadmap-2026-08.md), [designs/fragments-2026-09.md](designs/fragments-2026-09.md), [fragments-follow-ups.md](fragments-follow-ups.md) |
| Kotlin | [designs/kotlin-roadmap-2026-08.md](designs/kotlin-roadmap-2026-08.md), [designs/kotlin-shim-inventory.md](designs/kotlin-shim-inventory.md) |
| The simulator | [designs/freertos-host-sim.md](designs/freertos-host-sim.md) (amendments first), [designs/sim-pdb-endpoint-2026-09.md](designs/sim-pdb-endpoint-2026-09.md), [parity-audit.md](parity-audit.md) |
| `pdb`, the debug bridge | [designs/pdb-schema-as-code.md](designs/pdb-schema-as-code.md), [designs/sim-pdb-endpoint-2026-09.md](designs/sim-pdb-endpoint-2026-09.md) |
| Proposing new work | [quality-roadmap.md](quality-roadmap.md) and [code-health-audit-2026-07.md](code-health-audit-2026-07.md) §9 — it may already be listed, with the tradeoff written down |

## Live documents

Updated as work lands. These do not go stale by design; if one has, fix it.

| Doc | What it is |
|---|---|
| [parity-audit.md](parity-audit.md) | The simulator ↔ MCU divergence register. The rule it enforces: **every** divergence is a simulator bug, host-only headroom included. The most-linked doc in the repo — source comments cite its finding ids. |
| [quality-roadmap.md](quality-roadmap.md) | The standing backlog of deferred quality work, by theme (regression automation, test coverage, memory footprint, long-term stability). Each entry carries the tradeoff to weigh before starting. Where a "not now" goes so it is not lost. |
| [designs/android-parity-roadmap-2026-08.md](designs/android-parity-roadmap-2026-08.md) | The standing tracker for growing `picodroid.*` toward `android.*`. T-item table with open and partial rows (done rows in [completed/](completed/android-parity-roadmap-2026-08.md)); also records what is explicitly **not** being done. |
| [memory-diagnostics.md](memory-diagnostics.md) | Reference for the `mem-diag` feature — heap census, memmon, offensive checks. Copy-pasteable commands; zero cost when the feature is off. |
| [scheduling-diagnostics.md](scheduling-diagnostics.md) | Reference for the `sched-diag` feature — core hogs, sleep-polling, starvation, spin overruns. |
| [designs/kotlin-shim-inventory.md](designs/kotlin-shim-inventory.md) | Generated source of truth for the `kotlin/**` shim: every `(owner, name, desc)` kotlinc emits, with the fixture that caused it. Regenerate, do not hand-edit. |

## Open work

Plans and backlogs with items still open. Each lists **only open work**; closed items move to
a same-named file in [`completed/`](completed/), linked from the top of the original, whose
"Completed items" line keeps the moved ids greppable. Check the doc for the current row before
starting.

| Doc | Status | Left to do |
|---|---|---|
| [qa-2026-09-13-followups.md](qa-2026-09-13-followups.md) | nearly closed | Items 1–7 closed. Which cause drops item 1's SPI byte (and why the RP2040 is immune); an automated quota check for `SharedPreferences.commit()`; `qa_thr` off the W board; watch the touch kit's empty RTT capture and the RP2040's `pdb-install[shrink]` PING miss. |
| [bugs-rp2040-imagedemo-2026-09-15.md](bugs-rp2040-imagedemo-2026-09-15.md) | fixed | Unaligned papk asset pixels HardFaulted the RP2040 on any scaled image. Read it for the probe-rs `catch_hardfault` trap that hid the faulting PC for six nights. |
| [tls-follow-ups.md](tls-follow-ups.md) | open | TLS-4 flash levers, TLS-5 live `askclaude` run, TLS-6 `pdb prefs set`, TLS-7 handshake stack. TLS-1, TLS-2 and TLS-8 look done but are not yet marked closed. TLS-9 fork decision recorded. |
| [fragments-follow-ups.md](fragments-follow-ups.md) | open | FR-4 a swipe turning the pager, FR-9 what the model still lacks, FR-10 a bridge-backed row, FR-11 keys on button-less boards. FR-1, FR-2 (RP2040 reserve decision) and FR-6 look done but are not yet marked closed. |
| [networking-followups-2026-08.md](networking-followups-2026-08.md) | open | NET-1 (BCME -5 iovars), NET-8 (WPA3 built, needs verification against a real WPA3 AP), NET-3 (upstream bsscfg PR), NET-9 (socket I/O chunk size), NET-12 (about 1 power cycle in 30 never joins WiFi). |
| [upstream-cyw43-bsscfg-pr.md](upstream-cyw43-bsscfg-pr.md) | open, manual | NET-3. The patch is prepared; pushing and opening the PR are deliberately left to a human. Upstream v2.0.0 (tracked since 2026-09-16) still lacks the fix, and the branch merges cleanly onto it — see the amendment. |
| [scheduling-audit-2026-09.md](scheduling-audit-2026-09.md) | mostly landed | The 2026-09-12 busy-wait audit and its offensive-guard design. Open: WP7's timer-reprogramming half (waits on an event-driven indev), WP9's UART TX ring + IRQ, the G6 sched-diag HIL rows, WP0 (landed, needs verification: one HIL pass for the button and GT911 wakes), the W-slot `blinky pdb launch` stall. |
| [scheduling-audit-handover-2026-09.md](scheduling-audit-handover-2026-09.md) | open | The pick-up-cold companion to the above, with the WP7 investigation record; adds the GP11 edge measurement and the `IDLE_POLL_MS` raise. |
| [code-health-audit-2026-07.md](code-health-audit-2026-07.md) | P2 open | Four-axis repo audit, 2026-07-24. P0, P1 and P2 11, 12, 14, 17 closed. Open: 13 (jvm facade tests, `invoke_*` consolidation), 15 (crate READMEs), 16 (script hardening). |
| [code-health-2026-09.md](code-health-2026-09.md) | backlog open | The 2026-09-20 round. Its ten-item backlog is untouched: `pd-hal`/lvgl engine crates, back-stack transitions, test gaps, executors extraction, file splits, a coverage tool, hardware verification. |
| [designs/flash-budget-2026-09.md](designs/flash-budget-2026-09.md) | open levers | Where the flash bytes go after the string work. Open: §8 rows 4 (LVGL config), 5 (float formatting), 6 (SDK tree-shake + `LV_USE_*`), 7 (`no-pdb`), 9 (shared string table). |
| [designs/sram-hotpath-2026-09.md](designs/sram-hotpath-2026-09.md) | landed on every RP2350 board | Which code beyond the interpreter loop earns SRAM, measured with a DWT PC-sample profiler; option D (38 KB) cut a page turn's CPU by a third. What placement cannot fix is P1–P5 in `claudeusage-gaps-roadmap-2026-09.md`. |
| [designs/scroll-performance-2026-09.md](designs/scroll-performance-2026-09.md) | partly landed | S1, S3–S6 and S9 landed (a Set-time scroll frame 109 ms → 24 ms). Open: measure S2 on the touch kit, S6b (entry paint / DatePicker), S7 (tearing / TE pin), S8 (GP11), the 75 MHz SPI question. |
| [designs/rgb565-swapped-render-2026-09.md](designs/rgb565-swapped-render-2026-09.md) | built | LVGL renders straight into RGB565_SWAPPED; LV_COLOR_16_SWAP, which v10 removes, is gone. +6.5 KB flash, not faster; band bytes proven identical in sim and on the RP2350, RP2040 and pico_touch_kit. |
| [designs/psram-lvgl-fluid-scroll-2026-09.md](designs/psram-lvgl-fluid-scroll-2026-09.md) | partly landed | Stages 1–4 built 2026-09-12; the LVGL pool measured in PSRAM and left off (read the completed §4.1 before flipping the key). Open: §7 step 9 (pre-rendered page in PSRAM), Stage 5, a PSRAM tenant that pays. |
| [designs/app-store-roadmap-2026-09.md](designs/app-store-roadmap-2026-09.md) | partly landed | S0, S1, S5, S7 done; S2 and S6 partial (streaming `Session`, store app). S3 signing, S4 network install, S8 store, S9 permissions not built. |
| [designs/picoclock-roadmap.md](designs/picoclock-roadmap.md) | open | R1 (AlarmManager) done; R2–R15 open (R12 partial). |
| [designs/claudeusage-gaps-roadmap-2026-09.md](designs/claudeusage-gaps-roadmap-2026-09.md) | mostly closed | D1–D5, most G and H items closed. Open: D4's runtime residual, G5 alpha assets, G6 backlight PWM, G7 mDNS, G11, H3 (drop `java.time`), H10 (JVM task stack), minor items, P1–P5 performance. |
| [designs/claudeusage-android-shape-2026-09.md](designs/claudeusage-android-shape-2026-09.md) | round 1 landed | Every way the `claudeusage` app departed from Android idiom. Open: SDK ask 4 (one-shot timer), ask 7 BuildConfig (landed per G7 — status line stale), rows 14, 35, 39, 50. |
| [followups-2026-08.md](followups-2026-08.md) | nearly closed | Post GC-race backlog. Open: §3 (log the edit-mode key drop), §7 (GC-pacing device measurements), §8 (file the BME688 tickets). |
| [designs/psram-rp2350b-2026-09.md](designs/psram-rp2350b-2026-09.md) | partly built | 8 MB PSRAM on the RP2350B. Stages 1–4 built 2026-09-12 as an opt-in; Stage 5 (class bytes in PSRAM) not started. |
| [designs/meshtastic-roadmap-2026-08.md](designs/meshtastic-roadmap-2026-08.md) | not started | Nine sessions toward a board and app that are on-air compatible with stock Meshtastic. |
| [designs/edge-llm-roadmap-2026-08.md](designs/edge-llm-roadmap-2026-08.md) | not started | A language model on the MCU, across three tracks; track B would add a `platforms/stm32` family. Most of A1 now exists as `pico_touch_kit` — see the amendment. |

## Landed designs

Already built. Read these to understand the code, not to re-execute them.

### Architecture and the porting seam

| Doc | Landed |
|---|---|
| [designs/shared-core-extraction.md](designs/shared-core-extraction.md) | Moved ~26K lines of runtime out of the RP binary crate into `crates/picodroid-core`. Origin of the HAL traits + facade, `Rtos`, `PlatformHooks`, the registration macros and the shadow-twin rule. |
| [designs/family-neutral-residue.md](designs/family-neutral-residue.md) | The successor audit: ~4K of the lines left in `platforms/rp` were family-neutral, and moved. |
| [designs/porting-seam-2026-09.md](designs/porting-seam-2026-09.md) | S0–S10. `picodroid_core::porting` became the porting checklist, and the porting guide was rewritten around it. |
| [designs/network-seam-2026-09.md](designs/network-seam-2026-09.md) | The FreeRTOS + FreeRTOS+TCP seams, `NetLink`, `ConnectivityManager`. Merged and released as map v0.19.0 (`9b68fb7d`). |
| [designs/tls-2026-09.md](designs/tls-2026-09.md) | HTTPS: TLS 1.3 client on the vendored `embedded-tls` (`pd-tls`: compiled-in roots, multi-anchor verifier, heap-resident session), `HttpsURLConnection`, `SntpClient`, the `askclaude` and `https_get` examples, the nightly's TLS listener. RP2350 WiFi boards (`has_tls`). |
| [designs/wifi-provisioning-2026-09.md](designs/wifi-provisioning-2026-09.md) | 2026-09-26. Settings → Wi-Fi: scan, join, a network saved at `/system/wifi` and rejoined at boot, build-time credentials overriding it; `picodroid.net.wifi.WifiManager`; the system keyboard driven by four buttons. |

### JVM, memory and scheduling

| Doc | Landed |
|---|---|
| [designs/jvm-run-lock-2026-09.md](designs/jvm-run-lock-2026-09.md) | 2026-09-15. One interpreting task at a time, held by a kernel mutex instead of by scheduler configuration. New blocking paths must go through `unlocked()`. `give` is a hand-off (release, then yield) since 2026-09-25, for the simulator's single-core kernel. |
| [designs/handle-table-invalidation.md](designs/handle-table-invalidation.md) | Generation-tagged widget handles, so a deleted widget's handle cannot alias a live one. Every target runs the table since 2026-09-15; `legacy-handle-cast` restores the old pointer cast for one release. |
| [designs/method-level-native-registry.md](designs/method-level-native-registry.md) | Phase 1. `native_handler/method_tables.rs` plus the bidirectional test whose failure message prints ready-to-paste rows — that message *is* the workflow for adding a native. |
| [designs/value-slot-8b.md](designs/value-slot-8b.md) | 2026-09-17. `Value` 16 B → 8 B behind a `Slot` storage type for the fields arena, collection buffers and lambda captures; frames keep 16 B. Read "As built" at the end. |
| [designs/freertos-host-sim.md](designs/freertos-host-sim.md) | The simulator runs the real FreeRTOS kernel; `sim` *means* FreeRTOS, with no feature flag. **The body describes a two-backing world that no longer exists — read the amendments first.** |

### Graphics and input

| Doc | Landed |
|---|---|
| [designs/band-height-120-2026-09.md](designs/band-height-120-2026-09.md) | 2026-09-12. Taller draw bands on the touch board, with the measurement recipe in §6. Reshaped the same day by the async flush: the board spends the same bytes on two 60-row buffers. |
| [designs/canvas-2026-09.md](designs/canvas-2026-09.md) | 2026-09-26. `View.onDraw(Canvas)` as a retained display list replayed from LVGL's draw event: no pixel buffer, every board. Why not `lv_canvas` or `lv_chart`, and the LVGL-pool hang found on the way. |
| [designs/fragments-2026-09.md](designs/fragments-2026-09.md) | 2026-09-27. `picodroid.app.Fragment` / `FragmentManager` / `FragmentTransaction` driven from the Activity's trampolines, pure Java, and `ViewPager2` + `FragmentStateAdapter` over them with one page alive at a time; why Android added fragments and how each reason holds on a panel; what differs (views freed on `onDestroyView`, a `FragmentFactory` in place of reflection); `claudeusage`'s four pages moved onto it. |

### Networking

| Doc | Landed |
|---|---|
| [designs/cyw43-pio-transport.md](designs/cyw43-pio-transport.md) | 2026-08-14. The PIO gSPI transport that let WiFi run on core 1. Kept for the debugging history and recipes, which are the durable value. |
| [designs/net-typed-exceptions.md](designs/net-typed-exceptions.md) | `ConnectException`, `SocketTimeoutException`, `UnknownHostException` and friends across the whole `picodroid.net` stack. Throw through `throw_net_exception`. |

### Build, shrink and packaging

| Doc | Landed |
|---|---|
| [designs/unconditional-shrink-2026-09.md](designs/unconditional-shrink-2026-09.md) | 2026-09-02, map v0.17.0. `--shrink` means ProGuard semantics; run-time name translation retired in favour of `c::` consts. |
| [designs/flash-string-budget-2026-08.md](designs/flash-string-budget-2026-08.md) | **Superseded** by `flash-budget-2026-09.md`. Kept for the method and the before numbers. |
| [designs/papk-format-crate.md](designs/papk-format-crate.md) | The shared `no_std` `papk-format` crate, ending five independent declarations of one format. |
| [designs/pdb-schema-as-code.md](designs/pdb-schema-as-code.md) | 2026-07-27, all stages. PDB payload layouts moved into `pdb-protocol`. §3 records the sim endpoint it deferred — since built, below. |
| [designs/sim-pdb-endpoint-2026-09.md](designs/sim-pdb-endpoint-2026-09.md) | 2026-09-14. The simulator is a `pdb` device: one socket per sim, the real installer, park and reboot. |

### Apps and platform services

| Doc | Landed |
|---|---|
| [designs/multi-app-2026-09.md](designs/multi-app-2026-09.md) | M0–M3, released as map v0.22.0. A dynamic app region, a package directory, cross-package launch, and the launcher. Amendments A1–A5 at the bottom carry the divergences. |
| [designs/alarm-manager-2026-09.md](designs/alarm-manager-2026-09.md) | 2026-09-11. An alarm that outlives the app that set it, living outside every app alongside the package directory. |
| [designs/picoclock-2026-09.md](designs/picoclock-2026-09.md) | 2026-09-11. What `examples/picoclock` is and why it is shaped that way. |
| [designs/inject-annotations-2026-08.md](designs/inject-annotations-2026-08.md) | JSR-330 `@Inject` / `@Singleton` resolved at build time; nothing about the annotations reaches the device. Kotlin goes through kapt. |
| [designs/kotlin-roadmap-2026-08.md](designs/kotlin-roadmap-2026-08.md) | All eight sessions done; roadmap closed. Kotlin apps run on the unchanged class-file format with a hand-written `kotlin/**` shim. |

## Investigations and dated records

Closed. History, evidence and repro recipes — useful when a similar symptom returns.

| Doc | What happened |
|---|---|
| [qa-2026-09-13.md](qa-2026-09-13.md) | Seven self-checking `qa_*` apps written against the served API; forty defects found and fixed, one commit each. Open items moved to the followups doc above. |
| [bugbash-2026-08-30.md](bugbash-2026-08-30.md) | Repo-wide sweep: ~45 candidates, each turned into a failing test or repro app, then fixed or closed as not-a-bug. |
| [bugs-hil-nightly-2026-09-09.md](bugs-hil-nightly-2026-09-09.md) | Triage of the first three-slot HIL night. Two root causes explained all of it: the rp2040 running out of heap, and one dead USB device stalling every `probe-rs` launch on the host. |
| [bugs-rp2040-flash-2026-08-01.md](bugs-rp2040-flash-2026-08-01.md) | **Resolved 2026-08-10.** LittleFS I/O hangs and `pdb install` timeouts on the rp2040 — core 1 taking the tick inside the XIP-off window, plus `configRUN_MULTIPLE_PRIORITIES` and a dead per-core VTOR. |
| [bugs-memory-stress-2026-07-23.md](bugs-memory-stress-2026-07-23.md) | **All four fixed 2026-07-24.** PEM-1 lost clicks, PEM-2 append-only listener maps, PEM-3 heap fragmentation, PEM-4 alert churn. Each section carries the fix commit and the on-device verification. |
| [picoenvmon-qa.md](picoenvmon-qa.md) | The long-running picoenvmon QA record, including the 2026-08-17 heap-corruption investigation that ended in `0c1326d`. |
| [mem-session-2026-08.md](mem-session-2026-08.md) | Attributing picoenvmon's heap to code constructs with the heap census, then landing measurement-gated reductions (C1–C4). |
| [perf-campaign-2026-08.md](perf-campaign-2026-08.md) | Running log of the "measure, then climb" perf campaign, session by session. |
| [perf-memory-handover-2026-08.md](perf-memory-handover-2026-08.md) | The handover that started that campaign — everything perf- or memory-relevant left open by the WiFi showcase. |
| [picoenvmon-soak-plan-2026-08.md](picoenvmon-soak-plan-2026-08.md) | Partly superseded. §4's signal-triage table and §5's expected-noise list are still the reference; the execution premise is not. |
| [picoenvmon-soak-handover-2026-08.md](picoenvmon-soak-handover-2026-08.md) | The runbook to actually use for a picoenvmon soak. |

## Adding a doc

- Put designs and roadmaps in [`designs/`](designs/); audits, QA rounds, bug records and
  handovers at the top level of `docs/`.
- Date the filename when the content is a snapshot (`bugs-…-2026-09-09.md`,
  `scroll-performance-2026-09.md`). Leave it undated only for something genuinely standing.
- Give it a **status line directly under the title** and keep that line current. It is the
  first thing anyone reads, and the only defence against a finished design being re-executed.
- Say which commit the claims were checked against, and keep measurements next to the command
  that produced them.
- When reality diverges from a design, **append an amendment** rather than editing the body —
  the divergence is usually the most valuable thing in the doc.
- Superseding a doc? Leave it in place, add a "superseded by" line at the top, and point the
  successor back at it. Source comments and the website link to these paths — and to the finding
  ids inside them (`PEM-3`, `HAL-05`, …) — by name.
- A doc may be deleted only when its work is closed **and** nothing outside `docs/` cites its path
  or its ids (`git grep` both). Fix the remaining in-`docs/` links in the same commit; git history
  keeps the text.
- When a backlog item or roadmap stage closes, move its section (or table row) verbatim to the
  same-named file in [`completed/`](completed/), fix its relative links, and add its id to the
  original's "Completed items" line. Keep ids unchanged: source comments cite them by the
  original's path. Partial items stay whole in the original.
- Then add a row to this index.
