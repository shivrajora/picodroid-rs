# Completed: Open Follow-ups — post GC-race fix (2026-08-17)

Items closed out of [followups-2026-08.md](../followups-2026-08.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

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

**Fixed 2026-09-04 (`fix/dashboard-stall`)** — `NetworkManager` keeps the
accept loop on its own thread and posts the NTP + weather job to the
framework's shared background pool (`Executors.backgroundExecutor()`), whose
four workers were already resident and idle in this app. Their 4 KiB stacks
were too small for the job (hard fault on the first weather fetch; measured
4.7 KB deep with `pdb sysmon`), so the W board.toml gives them 6 KiB (+8 KiB
of boot heap, 23 % headroom accepted over the 25 % rule) against a second
`Thread`'s 16 KiB plus its own class state. A busy flag stops double posts, a 180 s ceiling re-arms a lost job,
and Refresh is a flag the next tick honours. Measured on the board: first
byte under 0.4 s through NTP timeouts and weather fetches (was ~13 s).
Original note below.

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
