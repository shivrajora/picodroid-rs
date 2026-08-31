# Open Follow-ups — post GC-race fix (2026-08-17)

Everything left open after the picoenvmon soak/corruption investigation
(`picoenvmon-qa.md` 2026-08-17 sections; fix `0c1326d`), other than the
nightly soak + PEM-3 retune, which have their own runbook:
`picoenvmon-soak-handover-2026-08.md`. Ordered by risk.

## 1. Sim `Thread.start` parallelism — same race class, no guards (P1-risk)

**Closed 2026-08-30.** Two findings superseded the premise: the simulator has
run the real FreeRTOS POSIX-port kernel since M7 (2026-07-28), so its tasks
are serialised by the kernel, not by host threads; and the bug bash's
`threadstress` soak (B0) ran clean without hooks. The hooks are installed
anyway (`sim_boot.rs`, concurrency-parity WP0) so no `AtomicSection` is a
no-op on one target and real on the other. The text below is the original
note.

The device fix works by suspending the FreeRTOS scheduler around compound
heap mutations (`pico_jvm::atomic_section`). The sim installs **no hooks**,
so its guards are no-ops — deliberately, because the host has no FreeRTOS.
But sim `Thread.start` children run as real OS threads against the same
shared `SharedJvmHeap`, and host threads have genuine parallelism plus
preemption at any instruction. If the sim does not serialize JVM-executing
threads by some other mechanism, it has the same corruption class the
device just fixed — worse, actually, because two threads can run
simultaneously.

Why it has not visibly bitten: unknown. Maybe a GIL-like serialization
exists in the sim's thread glue; maybe the race is just rare on fast host
cores. "The sim never reproduced the device corruption" is consistent with
either.

To do: read the sim's `Thread.start` / bg-worker glue (`hal/sim`,
`bg_worker.rs` sim arms) and establish which is true. If threads genuinely
interleave heap access: install atomic-section hooks backed by a global
`std::sync::Mutex` (enter = lock, exit = unlock; re-entrancy needs either a
recursive mutex or the same nesting counter the FreeRTOS path gets for
free). Then run the parity-strict sim soak (`sim-run.sh` networking row)
to confirm no deadlock — the guarded sections never block, so a plain
mutex should be safe.

## 2. Shared StringBuilder buffer (`sb_buf`) cross-thread aliasing

**Fixed by `896f691` (2026-08-18)** — every StringBuilder owns its buffer
(`sb_store.rs`). Original note below.

`ObjectHeap` keeps a **stack of shared StringBuilder buffers**
(`sb_stack`, `object_heap/mod.rs:519-571`); every Java `StringBuilder`
aliases this state. Safe only while each builder's append→`toString`
sequence never crosses a blocking native. The weather fetch violates the
spirit of that contract: HttpClient-style read loops do
`append → blocking recv → append` on the network thread while the main
thread runs its own builder cycles. The stack discipline survives *nested*
use, but two builders alive concurrently and non-nested will interleave
their bytes (child appends land in the main task's top-of-stack buffer).

This is data corruption (garbage strings), not heap-structure corruption —
the atomic-section fix does NOT cover it, since the block happens between
guarded operations by design.

To do: audit which Java code paths hold a builder open across a blocking
call (weather/NTP line parsing is the prime suspect; grep the app +
framework Java for `StringBuilder` near socket reads). Options, cheapest
first: (a) offensive-mode owner tag — stamp the current task id
(`mem_diag::task_id`) on `sb_push`, assert it matches on every
append/`toString`, panic on mismatch (turns a silent interleave into a
trap; ~5 lines); (b) per-thread `sb_stack` keyed by task id; (c) rewrite
the offending Java to finish strings before blocking. Do (a) first and let
the soak say whether (b)/(c) are needed.

## 3. Edit-mode key consumption is invisible (driver false-FAILs)

The Settings NumberPicker edit mode (`graphics/lvgl/edit_mode.rs`,
consulted in `events.rs:494-501`) can consume a key entirely — no Java
queue push, so none of the `key: code=...` dispatch lines added in
`beb0e3d`. Every soak cycle logs two false `FAIL ... no-dispatch-log`
entries for the X/Y presses inside Settings. One `pd_info!` in the
edit-mode consumption branch (e.g. `key: edit-mode consumed pin N`) makes
the last silent drop point observable and lets `scripts/soak/soak-lib.sh`
verify those presses too. Trivial; touch `events.rs` only, then the usual
sim smoke + pre-commit.

## 4. memmon cannot see child-executor GCs

**Fixed by `7b5589f` (2026-08-18)** — the counters moved to the heap-wide
`GcState`. Original note below.

Each `Thread.start` child builds its own `PicodroidNativeHandler`, so
`report_gc` from collections triggered in a child lands in the child's
counters — memmon's `gc=`/`freed=` columns silently miss them
(`perf-memory-handover-2026-08.md` §3). During the fatal soak this showed
up as `live` oscillating with `gc=+0`. Any perf conclusion drawn from
memmon under background threads is wrong until fixed.

Fix candidates: route `report_gc` through shared state (the new
cross-executor handler registry from `0c1326d` is a natural home — memmon
could sum over registered handlers), or have memmon read `GcState`
counters directly (single source of truth; GcState is already shared).
The second is probably smaller.

## 5. Serve-loop latency: NTP/weather block page loads

`NetworkManager` runs accept + housekeeping on one thread; boot NTP
retries (3 s × 3), weather fetch (DNS + HTTP ~5 s), the 6 h NTP re-sync
and 15 min weather refresh all stall the dashboard for seconds
(`perf-memory-handover-2026-08.md` §4). Options, cheapest first: shrink
NTP timeout/attempts; deadline-slice housekeeping (one attempt per tick);
a second thread (16 KiB stack + interacts with item 6 — measure first).
Unchanged by the GC-race fix except that a second thread is now *safe* to
consider.

## 6. Per-child duplicate class metadata — biggest heap lever

**Fixed by `807cc37` (2026-08-18)** — one class set shared across executors
(`boot::shared_jvm`). Original note below.

Every `Thread.start` child builds a fresh `Jvm` + `load_classes`
(`native_handler/os.rs`) — a full duplicate parsed-class set on the shared
arena for the thread's lifetime; the network thread makes one permanent
(`perf-memory-handover-2026-08.md` §6, unmeasured, likely tens of KB).
Sharing the immutable post-boot class set across executors is the single
largest recoverable heap win and cuts child spawn latency. Larger design
change; do it after the PEM-3 numbers exist so the win is measurable.

## 7. GC-pacing measurements (handover §2) — partially done

New data from this session: the first on-device benchmark baseline exists
— `Benchmark: TOTAL: 176,738 ms` on `pico_enviro_mon_w` release with
atomic sections (175,311 ms with empty-body hooks; binary layout swings
±5%, so treat single-build deltas under that as noise —
`picoenvmon-qa.md` measurement table). Still open from §2: pause
duration/frequency on device with `gc_alloc_threshold = 64`, whether 64
stays per-board or becomes byte-weighted, and the interpreter fold cost.

## 8. Hardware oddities — ticket, don't debug during soaks

- BME688 gas reads a constant 12,887,828 Ω: the heater profile is never
  programmed. IAQ tile/LED cosmetic.
- Pressure ~3600 hPa (raw `press=356850`): physically implausible,
  pre-existing driver/compensation artifact.

Both predate the networking work; both keep tripping people reading soak
logs. File as their own items so soak triage can keep ignoring them.

## 9. Housekeeping notes for whoever picks these up

- The offensive traps added in `0c1326d` (span/overlap invariants, root
  audit, task-tagged alloc trace) are permanent but offensive-gated — they
  cost nothing unless `PICODROID_MEMDIAG_OFFENSIVE=1` is baked at build
  time. Keep them; they are the reason the race was catchable.
- (Done with this doc: `memory-diagnostics.md` now documents device
  offensive arming; the `parked_frames` safety comment now cites
  `atomic_section`.)
