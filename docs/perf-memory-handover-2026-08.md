# Performance & Memory Handover — 2026-08-16 (post WiFi-showcase)

Scope: everything performance- and memory-relevant discovered, changed, or left
open during the picoenvmon WiFi showcase session (commits `34f97bb..e443805`:
new `pico_enviro_mon_w` board, dashboard/NTP/weather, plus five platform fixes).
This doc is the starting point for a dedicated perf/memory session.

Companion context: `docs/picoenvmon-qa.md` (2026-08-16 update + open panic),
`docs/memory-diagnostics.md` (tooling), `docs/bugs-memory-stress-2026-07-23.md`
(PEM history), project memory `project_jvm_concurrency_gc_fixes` /
`project_picoenvmon_wifi_showcase`.

---

## 1. P0 — device-only panic under combined stress (open)

The one genuine crasher. ~4 min into an on-device soak (Live screen + Logger on,
dashboard serving at 2 s cadence, pdb input churn), core 0 panicked:

```
panicked at core/src/slice/index.rs:1020:51:
range end index 388 out of range for slice of length 336
```

- Immediately preceded by routine `bme:` sampler debug lines; post-panic
  backtrace is the panic-probe hardfault, so the faulting frame is obscured.
- **Sim does not reproduce** the identical scenario (twice: Live+Logger+nav
  churn+45–100 paced requests). Classic sim-invisible class.
- Exonerated: `send_native`/`recv_native` (both copy through a stack buffer
  around the blocking call — no arena slice held across a yield).
- Prime suspects:
  1. An **arena/compaction span inconsistency** — a 336-element backing store
     addressed to 388 smells like a stale `(offset, len)` after
     `compact_fields_arena` / array-arena compaction. Latent path, now
     exercised ~10× more often by the GC pacing change (§2).
  2. **Sampler-task ↔ JVM heap interaction at real priorities** — the sensor
     sampler task (priority 6) runs only when all JVM tasks block, but a JVM
     task waking *preempts it mid-whatever* (15 > 6). Audit what the sampler
     path touches on the shared heap (the SensorEvent recycling from
     `project_native_alloc_gc_gap` reduced but may not have eliminated it).
- Recipe: reflash `--mem-diag` with `PICODROID_MEMDIAG_OFFENSIVE=1` — offensive
  mode poisons freed spans and panics **at damage time** (live object holding
  poison / post-GC integrity check), which converts this from a late symptom to
  a first-cause trap. gdb flow: `reference_gdb_sim_debugging` +
  `project_handle_dangle_sim_blind` memory notes (gdb-multiarch + probe-rs).
- Reproduction bias: crash needed Live UI churn + serving + input injection
  simultaneously; plain dashboard soaks (15/15 paced fetches, minutes) were
  clean before and after.

## 2. GC pacing rework — what changed, what's unmeasured

Two changes landed (commits `31c9a09`, `cf9f85d`):

1. **Native-minted allocations now count toward GC pacing.** Each heap
   (ObjectHeap, ArrayHeap, StringTable dyn-interns) bumps its own
   `alloc_events`; the interpreter folds them via `fold_native_alloc_events()`
   **after every invoke opcode** and at the 256-instruction checkpoint. The
   five per-opcode `bump_alloc_count` sites were removed (single funnel;
   parity-metrics semantics changed equally on both sides).
2. **`gc_alloc_threshold = 64` on `pico_enviro_mon_w`** (default 256): the
   server allocates few-but-large objects, which outran the count-based
   threshold byte-wise (§5 history).

Consequences to measure in the perf session:

- **GC pause frequency ~10× up on the W board** — and threshold 64 applies to
  *all* workloads on that board, UI nav included. Pause duration/frequency on
  device is unmeasured (`report_gc` provides time_ns; perfbench /
  `scripts/parity-bench.sh` exist).
- **Interpreter hot-path cost**: the fold is 3 counter reads/writes per invoke
  opcode. Never benchmarked before/after. Sim `benchmark` app ran 825 ms TOTAL
  *after* the scheduler fix but *before* the pacing change — no post-pacing or
  on-device comparison exists. Run `--app benchmark` + parity-bench pre/post
  `31c9a09` if regression is suspected.
- **Counting slack**: pure-bytecode alloc loops fold only at the 256-insn
  checkpoint (bounded lag ≤256 allocs); `intern_dyn_owned` counts dedup hits
  as alloc events (slightly eager GC). Both deliberate, both cheap to revisit.
- **Byte-blind threshold**: pacing still counts *allocations*, not *bytes*. A
  byte-weighted trigger (or arena-growth-pressure trigger) would fit server
  workloads better than a per-board magic 64 — candidate design item.

## 3. memmon observability gap — background-thread GCs are invisible

During the passing 360 KB gate run, `live` visibly oscillated (GC collecting)
while memmon windows reported `gc=+0 freed=+0`. Cause: each `Thread.start`
child builds its **own `PicodroidNativeHandler`**, so `report_gc` from
collections triggered in a child executor lands in the child's handler state,
not the memmon's. Any perf work that trusts memmon's `gc`/`freed`/pause
columns under background threads will be misled. Fix candidates: route
`report_gc` through shared state, or have memmon read `GcState` directly.

## 4. Serve-loop latency — NTP/weather block page loads

`NetworkManager` runs everything on one thread; `housekeeping()` (NTP, weather)
executes between accepts. Boot-time NTP against an unreachable/slow pool is
3 s × 3 attempts, weather adds DNS + HTTP — worst case ~10–14 s during which
the dashboard **does not accept connections** (observed: curl timeout at boot;
recurs at the 6 h NTP re-sync and 15 min weather refresh, and after every
failure backoff at 5 min cadence). Options, cheapest first:

1. Shrink NTP `TIMEOUT_MS`/attempts (3 s × 3 is generous for a UDP exchange).
2. Deadline-slice housekeeping: one NTP attempt or one weather fetch per tick,
   never both, resume next tick.
3. Second thread — costs a 16 KiB JvmChild stack + one more parked frame
   stack; interacts with §6 (per-child class metadata), so measure first.

## 5. Heap state — measured numbers (2026-08-16)

Static RAM (of 532,480 B / 520 KB; BSS includes the 416 KB heap_4 arena):

| Build | RAM used | headroom |
|---|---|---|
| `testbench_rp2350` helloworld (debug) | 522,568 | 9,912 |
| `testbench_rp2350w` helloworld (debug) | 527,712 | 4,768 (network Δ = **+5,144**) |
| `pico_enviro_mon` picoenvmon (release) | 506,468 | 26,012 |
| `pico_enviro_mon_w` picoenvmon (release) | 511,996 | **20,484** |
| `pico_enviro_mon_w` picoenvmon (debug) | 511,496 | 20,984 |

(The older "picoenvmon is ~30 KB short" figure in `docs/quality-roadmap.md`
predates these measurements — reconcile it when touching that doc.)

Heap gate (sim, host-modeled): `sim.sh -b pico_enviro_mon_w -a picoenvmon
-l 360` + 100-request curl loop → live 16–19 KB oscillating, `nused` flat
~318 KB, `lblk` 31,008 stable, min-ever-free 32,528, frag 383–397 ‰, zero OOM.
Before the pacing+lean-server fixes the same gate OOM'd on 21–24 KB
table-growth reallocs against a ~20 KB largest block — that failure shape
(doubling realloc vs fragmented arena) is the canonical regression signature
to watch for.

IP-stack share of the arena is board-tunable (`net_*` keys in board.toml;
`pico_enviro_mon_w`: descriptors 8, TCP 2048/2048, win segs 8; further rungs
down documented in the plan: 6 descriptors, 1460 B buffers, 4 segs).

**Pending device work** (blocked on / merged with §1):
- The full 15-min `--mem-diag` on-device soak (browser refresh + nav
  fragmenter) with go-criteria: no OOM, min-ever free ≥ 40 KB, `lblk` ≥ 16 KB
  at matched idle. Cut short by the panic at ~4 min.
- **PEM-3 prereserve retune**: `pico_enviro_mon_w`'s `[jvm] prereserve_*`
  values are copied from the plain board, sized *without* the network thread.
  Re-derive from on-device `memmon: storage` steady state under nav + serving.

## 6. Biggest heap lever — per-child duplicate class metadata

Every `Thread.start` child builds a **fresh `Jvm` + `load_classes`**
(`native_handler/os.rs`), i.e. a full duplicate set of parsed class metadata
on the shared arena for the life of the thread. The network thread makes one
such duplicate permanent. Size unmeasured (likely tens of KB for picoenvmon's
~50 classes). Sharing the parsed-class set across executors (it's immutable
after boot) is probably the single largest recoverable heap win, and also
cuts child-thread spawn latency.

## 7. Residual concurrency holes (correctness-adjacent perf)

- **Unequal-priority JVM work can still race**: `configUSE_TIME_SLICING = 0`
  fixed equal-priority slicing, but the background-executor pool runs at
  priority 1–10 — a waking jvm_task (15) preempts a pool worker
  **mid-heap-mutation**. Short jobs make it rare (executordemo passes
  nightly), but it is the same corruption class as the fixed bug. Proper fix:
  one priority tier for every interpreting task, or critical sections around
  heap mutation. Note Android `setPriority` semantics become advisory either way.
- **Shared StringBuilder buffer** (`sb_buf`, ObjectHeap): all StringBuilders
  alias one buffer, safe only because append→toString sequences never cross a
  blocking native. Nothing enforces that; a Java method that logs (or blocks)
  mid-build from two threads would interleave. Audit-worthy.
- **Time-slicing-off fairness**: a compute-bound Java thread now holds the CPU
  until it blocks — equal-priority starvation is possible by design. Fine for
  the server (blocks constantly); user apps with busy loops will starve the UI.

## 8. Minor known costs (recorded, low priority)

- Serve path per request: ~250 B dynamic page middle + StringBuilder chain +
  one log line on 404s; response heads and page frame are cached constants;
  `pageBuf` (1536 B) is persistent. Uptime/clock make every page unique — dyn
  strings are interned and become garbage each request (bounded by GC).
- Wall-clock seqlock reads spin only during a concurrent write (rare: NTP
  every 6 h); writer is single-by-contract.
- `parked_frames` registry: one Vec push/pop per `execute()`, one extra walk
  per GC — negligible, but each parked stack extends mark time slightly.
- The GC-trigger check now folds only on invoke opcodes + checkpoint; if the
  fold shows up in profiles, checkpoint-only (256-insn) folding is the fallback
  with threshold headroom adjusted.

## 9. Suggested attack order for the perf/memory session

1. **§1 panic** — offensive-mem-diag reflash, drive the same combined load,
   let the poison trap identify the writer. Everything else in §5 (soak,
   prereserve retune) rides on the same reflash.
2. **§3 memmon gap** — fix observability first or the soak numbers lie.
3. **§2 measurements** — benchmark + parity-bench before/after `31c9a09` on
   sim and device; decide whether threshold 64 stays, moves per-workload, or
   becomes byte-weighted.
4. **§4 serve-loop latency** — cheap wins (timeouts/slicing) unless
   measurements justify a second thread.
5. **§6 class-metadata sharing** — the big heap lever, larger design change.
6. **§7 holes** — schedule as their own correctness item.

Verification commands used this session (all still valid):

```bash
# sim heap gate under load
./scripts/sim.sh -b pico_enviro_mon_w -a picoenvmon -l 360 -m
for i in $(seq 1 100); do curl -s http://127.0.0.1:8080/ >/dev/null; sleep 1; done

# device flash with creds (flash.sh never exits — background it)
env $(grep -v '^#' .wifi-creds.env | xargs) \
  ./scripts/flash.sh --board pico_enviro_mon_w --app picoenvmon --release

# offensive mem-diag device build (mem-diag is a Cargo feature on device —
# flash.sh has no -m flag; PICODROID_EXTRA_FEATURES appends it)
env $(grep -v '^#' .wifi-creds.env | xargs) \
  PICODROID_EXTRA_FEATURES=mem-diag PICODROID_MEMDIAG_OFFENSIVE=1 \
  ./scripts/flash.sh --board pico_enviro_mon_w --app picoenvmon --release

# input injection for the combined-load repro (device via pdb, sim via stdin)
./scripts/pdb.sh input keyevent 23   # ENTER; 19=PREV 20=NEXT 4=ESC
```
