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
| LVGL, drawing, scrolling, touch | [designs/scroll-performance-2026-09.md](designs/scroll-performance-2026-09.md), [designs/band-height-120-2026-09.md](designs/band-height-120-2026-09.md), [designs/psram-lvgl-fluid-scroll-2026-09.md](designs/psram-lvgl-fluid-scroll-2026-09.md), [designs/handle-table-invalidation.md](designs/handle-table-invalidation.md) |
| Networking | [designs/network-seam-2026-09.md](designs/network-seam-2026-09.md), [designs/net-typed-exceptions.md](designs/net-typed-exceptions.md), [networking-followups-2026-08.md](networking-followups-2026-08.md), [designs/cyw43-pio-transport.md](designs/cyw43-pio-transport.md) |
| A new board, or a new MCU family | [designs/porting-seam-2026-09.md](designs/porting-seam-2026-09.md), [designs/shared-core-extraction.md](designs/shared-core-extraction.md), [designs/family-neutral-residue.md](designs/family-neutral-residue.md), and the published [porting guide](../website/src/content/docs/reference/porting-guide.md) |
| Flash or RAM budget, the shrinker | [designs/flash-budget-2026-09.md](designs/flash-budget-2026-09.md), [designs/unconditional-shrink-2026-09.md](designs/unconditional-shrink-2026-09.md), [designs/value-slot-8b.md](designs/value-slot-8b.md) |
| Packages, install, the launcher | [designs/multi-app-2026-09.md](designs/multi-app-2026-09.md), [designs/app-store-roadmap-2026-09.md](designs/app-store-roadmap-2026-09.md), [designs/alarm-manager-2026-09.md](designs/alarm-manager-2026-09.md) |
| The Java API surface | [designs/android-parity-roadmap-2026-08.md](designs/android-parity-roadmap-2026-08.md) |
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
| [designs/android-parity-roadmap-2026-08.md](designs/android-parity-roadmap-2026-08.md) | The standing tracker for growing `picodroid.*` toward `android.*`. T-item table with open, partial and done rows; also records what is explicitly **not** being done. |
| [memory-diagnostics.md](memory-diagnostics.md) | Reference for the `mem-diag` feature — heap census, memmon, offensive checks. Copy-pasteable commands; zero cost when the feature is off. |
| [scheduling-diagnostics.md](scheduling-diagnostics.md) | Reference for the `sched-diag` feature — core hogs, sleep-polling, starvation, spin overruns. |
| [designs/kotlin-shim-inventory.md](designs/kotlin-shim-inventory.md) | Generated source of truth for the `kotlin/**` shim: every `(owner, name, desc)` kotlinc emits, with the fixture that caused it. Regenerate, do not hand-edit. |

## Open work

Plans and backlogs with items still open. Check the doc for the current row before starting.

| Doc | Status | Left to do |
|---|---|---|
| [qa-2026-09-13-followups.md](qa-2026-09-13-followups.md) | nearly closed | Item 2 (the `handle-table-32` soak) is the only item still open; item 1 was found and fixed 2026-09-15, items 3–8 landed 2026-09-14. §11's `imagedemo` ERROR and item 3's RP2040 10 KB are both fixed (2026-09-15); the touch kit's 7680 B is the last infallible native allocation. |
| [bugs-rp2040-imagedemo-2026-09-15.md](bugs-rp2040-imagedemo-2026-09-15.md) | fixed | Unaligned papk asset pixels HardFaulted the RP2040 on any scaled image. Read it for the probe-rs `catch_hardfault` trap that hid the faulting PC for six nights. |
| [networking-followups-2026-08.md](networking-followups-2026-08.md) | open | NET-10 (dashboard loads hang, RST-on-close) open since 2026-09-07; NET-1 and NET-9 leftovers remain. NET-2/4/5/6/7/8 done. |
| [upstream-cyw43-bsscfg-pr.md](upstream-cyw43-bsscfg-pr.md) | open, manual | NET-3. The patch is prepared; refreshing against upstream, pushing and opening the PR are deliberately left to a human. |
| [scheduling-audit-2026-09.md](scheduling-audit-2026-09.md) | mostly landed | The 2026-09-12 busy-wait audit and its offensive-guard design. Status table at the top: WP0 is open; everything else landed, WP7 (tick timebase) on its second landing, 2026-09-15. |
| [scheduling-audit-handover-2026-09.md](scheduling-audit-handover-2026-09.md) | open | The pick-up-cold companion to the above: what is left, with the WP7 investigation record. WP7 re-landed 2026-09-15 once the SPI stall under it and its own RTC-alarm gate bug were fixed. |
| [code-health-audit-2026-07.md](code-health-audit-2026-07.md) | P2 open | Four-axis repo audit (coverage, modularization, reusability, API contracts), 2026-07-24. §9 backlog: P0 and P1 closed, P2 items 11–17 open. |
| [designs/flash-budget-2026-09.md](designs/flash-budget-2026-09.md) | open levers | Where the flash bytes go after the string work. `opt-level = "s"` is measured at −23.8 % and unadopted pending a HIL benchmark; C at `-Os` and the `c::` consts landed. |
| [designs/scroll-performance-2026-09.md](designs/scroll-performance-2026-09.md) | partly landed | S1, S3, S4 and S5 landed (a Set-time scroll frame went 109 ms → 24 ms); S6 measured and found nothing; the rest are ranked and open. |
| [designs/psram-lvgl-fluid-scroll-2026-09.md](designs/psram-lvgl-fluid-scroll-2026-09.md) | partly landed | Stages 1–4 built 2026-09-12. §4.1 measured the LVGL pool in PSRAM and left it switched off — read it before flipping the key. |
| [designs/app-store-roadmap-2026-09.md](designs/app-store-roadmap-2026-09.md) | partly landed | S0, S2 (queries), S6 (launcher) and S7 arrived via the multi-app work. Image signing, the streaming install `Session` and a store itself are not built. |
| [designs/picoclock-roadmap.md](designs/picoclock-roadmap.md) | open | Everything unstarted except R1 (AlarmManager), kept for what it left behind. |
| [followups-2026-08.md](followups-2026-08.md) | nearly closed | Post GC-race backlog. Items 1–6 closed; item 7 (GC-pacing measurements) partly open. |
| [designs/value-slot-8b.md](designs/value-slot-8b.md) | designed, not started | `Value` 16 B → 8 B behind a `Slot` storage type — the largest memory lever left. Feasibility verdict, saving, risks, stages. |
| [designs/psram-rp2350b-2026-09.md](designs/psram-rp2350b-2026-09.md) | designed, not started | 8 MB PSRAM on the RP2350B. Its "nothing urgent" verdict is partly superseded — scroll profiling since found RAM to be the binding constraint. |
| [designs/meshtastic-roadmap-2026-08.md](designs/meshtastic-roadmap-2026-08.md) | not started | Nine sessions toward a board and app that are on-air compatible with stock Meshtastic. |
| [designs/edge-llm-roadmap-2026-08.md](designs/edge-llm-roadmap-2026-08.md) | not started | A language model on the MCU, across three tracks; track B would add a `platforms/stm32` family. |

## Landed designs

Already built. Read these to understand the code, not to re-execute them.

### Architecture and the porting seam

| Doc | Landed |
|---|---|
| [designs/shared-core-extraction.md](designs/shared-core-extraction.md) | Moved ~26K lines of runtime out of the RP binary crate into `crates/picodroid-core`. Origin of the HAL traits + facade, `Rtos`, `PlatformHooks`, the registration macros and the shadow-twin rule. |
| [designs/family-neutral-residue.md](designs/family-neutral-residue.md) | The successor audit: ~4K of the lines left in `platforms/rp` were family-neutral, and moved. |
| [designs/porting-seam-2026-09.md](designs/porting-seam-2026-09.md) | S0–S10. `picodroid_core::porting` became the porting checklist, and the porting guide was rewritten around it. |
| [designs/network-seam-2026-09.md](designs/network-seam-2026-09.md) | The FreeRTOS + FreeRTOS+TCP seams, `NetLink`, `ConnectivityManager`. Merged and released as map v0.19.0 (`9b68fb7d`). |

### JVM, memory and scheduling

| Doc | Landed |
|---|---|
| [designs/jvm-run-lock-2026-09.md](designs/jvm-run-lock-2026-09.md) | 2026-09-15. One interpreting task at a time, held by a kernel mutex instead of by scheduler configuration. New blocking paths must go through `unlocked()`. |
| [designs/handle-table-invalidation.md](designs/handle-table-invalidation.md) | Generation-tagged widget handles, so a deleted widget's handle cannot alias a live one. The 32-bit device arm sits behind the `handle-table-32` feature. |
| [designs/method-level-native-registry.md](designs/method-level-native-registry.md) | Phase 1. `native_handler/method_tables.rs` plus the bidirectional test whose failure message prints ready-to-paste rows — that message *is* the workflow for adding a native. |
| [designs/freertos-host-sim.md](designs/freertos-host-sim.md) | The simulator runs the real FreeRTOS kernel; `sim` *means* FreeRTOS, with no feature flag. **The body describes a two-backing world that no longer exists — read the amendments first.** |

### Graphics and input

| Doc | Landed |
|---|---|
| [designs/band-height-120-2026-09.md](designs/band-height-120-2026-09.md) | 2026-09-12. Taller draw bands on the touch board, with the measurement recipe in §6. Reshaped the same day by the async flush: the board spends the same bytes on two 60-row buffers. |

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
| [nightly-networking-handover.md](nightly-networking-handover.md) | **Landed 2026-09-04.** Making the 4 AM on-device nightly exercise the WiFi stack, so a network regression turns an email red. |

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
  successor back at it. Nothing here is deleted; source comments and the website link to these
  paths by name.
- Then add a row to this index.
