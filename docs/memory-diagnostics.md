# Memory Diagnostics (`mem-diag`)

Opt-in instrumentation for hunting heap growth, churn, and corruption on the
low-RAM targets (RP2040: 160 KB heap / 896 KB flash program region; RP2350:
408 KB heap). Everything here is gated behind the `mem-diag` cargo feature —
**when the feature is off, none of this code exists in the binary** (verified
byte-identical flash + RAM against a non-diag build; see "Zero-cost
guarantee" below).

Audience: developers and AI agents. Every command below is copy-pasteable.

## Enabling

| Where | How |
|---|---|
| Simulator | `./scripts/sim.sh --app <app> --mem-diag` (alias `-m`) |
| Firmware (RP2350) | `PICODROID_EXTRA_FEATURES=mem-diag ./scripts/flash.sh -b testbench_rp2350 -a <app>` |
| Firmware (RP2040) | Same, but **manual opt-in only** — the diag image lands ~0.1 KB under the 896 K program region. Never make it a default. |
| On-demand snapshot (sim) | `./scripts/sim-ctrl.sh memstats` against a running `sim-remote` (or write `memstats` to the control FIFO / stdin) |
| On-demand census (sim) | `./scripts/sim-ctrl.sh heapcensus` (or write `heapcensus` to the control FIFO / stdin) — the live-set census, see "The heap census" below |
| Device query | `./scripts/pdb.sh sysmon` — mem-diag firmware appends a JVM block (live bytes, post-GC floor, alloc total, largest free block) to the standard response |
| Soak suite | `./scripts/test-memdiag.sh` (also runs as a `sim-run.sh` lane) |

Runtime toggles within a `--mem-diag` sim build (all read once at startup):

| Env var | Default | Effect |
|---|---|---|
| `PICODROID_MEMDIAG_WINDOW_MS` | `1000` | Monitor window length (min 16 ms; keep ≥ 500 ms) |
| `PICODROID_MEMDIAG_SENTINEL` | `1` (set by `--mem-diag`) | Growth sentinel on/off |
| `PICODROID_MEMDIAG_STRICT` | off | Sentinel trip → `abort()` (turns soaks into hard failures; sim only — the device only warns) |
| `PICODROID_MEMDIAG_OFFENSIVE` | off | Poison-on-free + GC poison check + post-GC integrity sweep + span/overlap invariants + root audit; sim reads it at runtime, the device bakes it at BUILD time (`mem_diag::apply_device_flags` since `0c1326d` — before that it was silently sim-only; boot must print `memmon: offensive checks ON (build-baked)`). Allocator canaries remain sim-only |
| `PICODROID_MEMDIAG_HISTO` | off | Per-class allocation histogram (sim only) |
| `PICODROID_MEMDIAG_SELFTEST` | off | Feed the sentinel a synthetic +2 KB/window ramp — must print `LEAK?` (detector self-test; sim only) |
| `PICODROID_MEMDIAG_SITES` | off | Allocation-site ledger: every live arena block keeps the call stack that allocated it, and `heapcensus` attributes the whole arena by site and by stack (sim only; see "Attributing the rest of the arena") |
| `PICODROID_MEMDIAG_SITES_MIN` | `0` | With `_SITES`: smallest block traced, in bytes; smaller ones are counted, not attributed |
| `PICODROID_MEMDIAG_SITES_TOP` | `24` | With `_SITES`: rows printed per ranking (sites, stacks) |
| `PICODROID_MEMDIAG_CLASSDUMP` | off | `heapcensus` lists every parsed class with its metadata cost, not only the top 12 (sim only) |

On device there are no env vars: compiled-in = monitor active with the
defaults (1 s window, sentinel warn-only, no offensive checks). A mem-diag
image always logs `memdiag: ACTIVE` at startup — a production capture
containing that line is a diag build.

## Reading the output

One line per window, greppable by `memmon` (sim `[memmon] ...`, device RTT
`memmon: ...`):

```text
[memmon] w=12 live=2331 obj=2216 arr=0 str=115 floor=2331 nused=126856 nfree=299128 nmin=296664 lblk=295840 gc=+0 freed=+0 gcb=+0 alloc=+14 nalloc=+0 stri=+9 frag=11pm
```

| Field | Meaning |
|---|---|
| `w` | Window index (1 window ≈ `WINDOW_MS`) |
| `live` / `obj` / `arr` / `str` | JVM live bytes: total / ObjectHeap / ArrayHeap / dynamic strings (pointer-free layout — sim and device figures are directly comparable) |
| `floor` | **Post-GC live floor** for the window — the leak signal. Excludes not-yet-collected garbage; before the first GC it equals raw live (exact while nothing was freed) |
| `nused` / `nfree` | Native (FreeRTOS heap_4) used / free bytes |
| `nmin` | Lowest-ever native free (high-water complement) |
| `lblk` | Largest single free block |
| `gc=+N` / `freed=+N` | GC cycles / heap entries reclaimed this window. Counted on the heap-wide `GcState`, so collections run by `Thread.start` children are included (before this the columns came from the main executor's handler and read `+0` under background-thread churn — handover §3) |
| `gcb=+N` | Live bytes reclaimed by GC this window (pre-sweep minus post-sweep) — the byte-level companion to `freed`'s entry count, and the evidence base for a future byte-weighted GC trigger |
| `alloc=+N` | JVM allocations via bytecode this window |
| `nalloc=+N` | JVM allocations by native glue (lifecycle/sensor code) this window |
| `stri=+N` | `intern_dyn` calls this window (StringBuilder.toString / format / concat all sink here) |
| `frag=Npm` | Permille of free space **not** in the largest block — 0 = unfragmented, high values mean a big allocation may fail despite ample total free |

Special lines:

- `[memmon] LEAK? live floor rose +NB over 8 windows (baseline B, now N)` —
  the growth sentinel tripped (also emitted for the native floor with
  `native` in place of `live`). In strict mode the run aborts right after.
- `[memmon] GC-PRESSURE N GCs this window (...)` — ≥ 10 GCs in one window:
  heavy churn even if `live` stays flat. Find the source with the histogram.
- `[memmon] snapshot ...` — on-demand (`memstats`) or exit summary; same
  fields, cumulative counters instead of deltas.
- `[memmon] histo top: benchmark/Counter=50000 ...` — top-8 allocating
  classes (requires `_HISTO=1`); the "WHO is churning" answer.

## The heap census (`heapcensus`, sim)

Where the histogram answers "who is churning" (cumulative alloc counts since
boot), the census answers **"who is holding the bytes right now"** — a
live-set snapshot, attributed to code constructs. Printed with every
`memstats`/exit snapshot and on demand via `heapcensus`:

```text
[memmon] census obj: n=44 bytes=3312 classes=25
[memmon] census obj top: picodroid/hardware/Sensor=5n/540B picoenvmon/data/SensorRingBuffer=5n/380B ...
[memmon] census arr: ref=7n/792B(inl 5) float=12n/1680B(inl 7) byte=8n/10824B(inl 0) ... dead=0B slack=480B
[memmon] census str: dyn n=32 len=667 cap=677 slack=10 buckets=14/10/8/0/0/0 top_cap=39/39/38/38
[memmon] census side: lists=1n/64B maps=0n/0B sb=0B lambda=1n/16B exc=0B
[memmon] census classmeta main: 62/161 parsedB=92255 devB~=49387 tableB~=3240
[memmon] census classmeta child picoenvmon/net/NetworkManager: 11/161 parsedB=16123 devB~=8735 tableB~=5140
```

- `census obj` / `obj top` — live objects bucketed by class, `count`n/`bytes`B
  (slot + field span, the `live_bytes` accounting), top-8 by retained bytes.
- `census arr` — live arrays by element type; `(inl N)` = arrays small enough
  to live inline in the 40 B slot (no arena payload). `dead` = swept arena
  spans awaiting compaction, `slack` = reserved-but-unwritten arena capacity.
- `census str` — dynamic strings: logical `len` vs pinned `cap` (`slack` is
  StringBuilder growth slack carried into the interned buffer), a length
  histogram (≤16/≤32/≤64/≤128/≤256/>256), and the 4 largest capacities.
- `census side` — bytes the `live=` figure does **not** include: ArrayList
  (`lists`) and HashMap/HashSet (`maps`) backing buffers, StringBuilder text
  (`sb`), lambda captures, exception tables.
- `census classmeta` — per-executor parsed-class metadata: `parsed/total`
  classes, `parsedB` = bytes in this process (what the sim arena pays),
  `devB~` = 32-bit release re-derivation (4 B usize, 12 B Vec headers, heap_4
  block headers) — **use `devB~` for device sizing decisions**, the host
  figure is ~2× inflated by pointer width. `tableB~` = the registration
  table itself. One `child` row per live `Thread.start`/bg-pool executor
  (each child's parsed set is a full duplicate of the main one — the
  handover §6 lever; children register via `mem_diag::register_child_jvm`).
- `census classmeta parts host/dev` — the same bytes by the part of the
  parsed record that holds them: the `Box<Parsed>` itself, the constant-pool
  offset and tag tables, the method, field, static, interface and bootstrap
  tables, the exception tables. Says which packing lever pays: on
  claudeusage the CP offsets (`usize` per entry) and the method table are
  three quarters of it.
- `census classmeta classes` — the price of an import: one line per parsed
  class, most expensive first (top 12; all of them under
  `PICODROID_MEMDIAG_CLASSDUMP=1`), as `name=devB~/hostB
  cp<entries>/m<methods>/f<fields>/x<exception entries> <class file bytes>`.
  A class costs RAM in proportion to its constant pool and method count,
  not its bytecode.

### Attributing the rest of the arena (`PICODROID_MEMDIAG_SITES=1`)

`nused` minus the census above is a number with no owner: task stacks,
executor state, resolution tables, the file system, native side tables.
With `PICODROID_MEMDIAG_SITES=1` the simulator's allocator records the call
stack behind every live arena block (one unwind per allocation, a few
microseconds; symbolised only at census time), and `heapcensus` appends:

```text
[memmon] census native: live blocks=634 payload=393497B heap4=399408B (header+align tax 5911B) untraced<0B: 0n/0B (cumulative)
[memmon] census native sizes: <=16=173n/1303B <=32=53n/1359B ... <=4K=64n/129952B >4K=18n/181376B
[memmon] census native sites: 61 distinct, top 24
[memmon]    120976B    20n  picodroid_core::hal::sim::boot_budget::model::charge (boot_budget.rs:128)
[memmon]     74048B    71n  pico_jvm::class_file::parse::<impl pico_jvm::class_file::Parsed>::parse (parse.rs:400)
[memmon]     13824B     4n  pico_jvm::resolve_cache::Table<S>::ensure (resolve_cache.rs:164)
...
[memmon] census native stacks: 275 distinct, top 24
[memmon]     16504B     1n  ...boot_budget::model::charge (boot_budget.rs:128) <- ...charge_task_spawn <- ...rtos_freertos::spawn <- ...threads::thread_start0 (threads.rs:161) <- ...
```

- `census native` — every live block the arena holds: payload requested,
  what heap_4 charges for it (8 B header, 8 B alignment), and how much
  fell under `_SITES_MIN`.
- `census native sizes` — the live set by size class. Many tiny blocks
  mean header tax and fragmentation; the `>4K` row is stacks and tables.
- `census native sites` — bytes and blocks by the innermost frame inside
  this code base (runtime plumbing — `alloc`, `core`, `std`, the allocator
  — is skipped). The site that owns the bytes.
- `census native stacks` — the same by full call path, five of our frames
  deep: which caller reached the site (the `thread_start0` row above is one
  Java thread's stack and TCB).

The figures are host bytes: pointer-sized fields cost twice what they do
on the device, so a `Vec<usize>` site reads 2× and a task stack reads 1×.
Translate with the `devB~` ratio for class metadata and by inspection for
the rest, or read the device's own `nused` from a mem-diag firmware
(`pdb.sh sysmon`, or the RTT `memmon:` line) beside it. On claudeusage the
simulator charged 79 KB more than the RP2350 measured for the same screen
(docs/designs/claudeusage-gaps-roadmap-2026-09.md, G11).

## The growth sentinel

Watches two floors per window: the **post-GC JVM live floor** and the
**native used floor**. Arms after the first Activity's `onCreate` completes
(construction growth is legitimate) plus 2 settle windows, then trips when,
over the last **8** windows, ≥ 7 deltas are rising AND the rise across the
ring and above the armed baseline both exceed **4096 B** (two fields-arena
growth steps of 256 × 8 B slots; one step of 16 B `Value`s before
2026-09-17). A single-step rise that then stays flat (a lazily-built cache)
never trips; a persisting leak re-trips every 8 windows.

The **native** sentinel additionally re-baselines on every Activity push and
pop (2 fresh settle windows, then a new baseline): raw native use
legitimately steps with each screen's construction and teardown, and judging
it against the first Activity's baseline made every later screen a false
`LEAK?` on multi-screen apps. The cost is a deliberate ~3-window blind spot
around each transition — acceptable for a steady-state drift detector. The
JVM sentinel keeps its original arm-time baseline: its post-GC floor input
does not step at transitions, so it stays maximally sensitive.

The same contract in unit form: `gc_stress_steady_state_flat`
(`jvm/src/gc/tests.rs`) and the `rearm_*` cases in `jvm/src/mem_diag.rs`.
The end-to-end detection path — both channels — is exercised by the
`PICODROID_MEMDIAG_SELFTEST=1` cases in `scripts/test-memdiag.sh`.

## Offensive mode (`PICODROID_MEMDIAG_OFFENSIVE=1`, sim)

Fail-fast checks that catch corruption at the moment of damage instead of
letting it surface later as an unrelated hang:

- **Poison-on-free** — freed fields-arena spans and array payloads are
  filled with `0x5AFEDEAD`; freed dynamic-string buffers are scribbled
  `0xDE` before dropping (dangling-`&str` bugs show garbage, not
  plausibly-valid stale text).
- **GC poison check** — a *live* object field holding the poison pattern
  panics with the object and class (use-after-free / arena-compaction bug).
- **Post-GC integrity sweep** — span bounds + overlap for objects and
  arrays, `first_free` consistency, chunked-slot invariants, string-table
  ptr/len agreement. Panics with the violated invariant.
- **Allocator canaries** — every sim heap_4 allocation carries a trailing
  `0xDEADC0DE` word past its requested size, verified on free; a smashed
  canary (buffer overrun — LVGL C code included) aborts.

Device builds never enable offensive mode (`debug_asserts` are already
stripped there for flash; a diagnostic must not halt a device).

## Typical workflows

Watch an app idle and confirm it is allocation-flat:

```bash
./scripts/sim.sh --app myapp --mem-diag
# healthy steady state: alloc=+0 nalloc=+0 stri=+0 gc=+0, flat floor
```

Find who is churning:

```bash
PICODROID_MEMDIAG_HISTO=1 ./scripts/sim.sh --app myapp --mem-diag
./scripts/sim-ctrl.sh memstats        # prints the top-8 allocating classes
```

Hard-fail a soak on any steady-state growth:

```bash
PICODROID_MEMDIAG_STRICT=1 timeout 300 ./scripts/sim.sh --app myapp --mem-diag
# exit 124/143 = survived flat; SIGABRT = the sentinel tripped
```

Hunt heap corruption:

```bash
PICODROID_MEMDIAG_OFFENSIVE=1 ./scripts/sim.sh --app myapp --mem-diag
```

Attribute the whole arena — who holds the bytes `live=` does not cover:

```bash
PICODROID_MEMDIAG_SITES=1 PICODROID_MEMDIAG_CLASSDUMP=1 ./scripts/sim.sh --app myapp --mem-diag
./scripts/sim-ctrl.sh heapcensus      # sites, stacks, size classes, every parsed class
```

On-device numbers over USB (mem-diag firmware):

```bash
PICODROID_EXTRA_FEATURES=mem-diag ./scripts/flash.sh -b testbench_rp2350 -a myapp -r
./scripts/pdb.sh sysmon    # standard stats + the JVM mem-diag block
```

## Zero-cost guarantee

`mem-diag` is compile-time gated by feature absence — off means the code
does not exist, not "runtime-checked off":

- Firmware builds pass `--no-default-features`; the feature enters only via
  an explicit `PICODROID_EXTRA_FEATURES=mem-diag`. `sim.sh` only adds it
  with `--mem-diag`. Nothing enables it transitively.
- Verified: RP2350 debug firmware without the feature is **byte-identical**
  in flash and RAM to the pre-mem-diag baseline. With the feature:
  +6.2 KB flash, +4 B static RAM.
- The two seams living in always-compiled code are themselves cfg-gated:
  the CMD_SYSMON response keeps today's exact wire format without the
  feature, and the sim `memstats` command answers "built without mem-diag".
- `scripts/pre-commit` enforces both: a `sim,mem-diag` clippy pass and an
  RP2350 firmware build with the feature (link + flash budget).

## Contributor rules

- **No `fetch_add`/CAS on any RP2040-reachable path** — thumbv6-M has no
  atomic RMW instructions. Counters are plain non-atomic fields on
  single-threaded owners (`GcState.alloc_total`, the monitor state), like
  the existing `GcState.alloc_count`. Cross-core publishing uses `AtomicU32`
  `load()`/`store()` only (the `ACTIVE_JVM_THREADS` discipline in
  `pdb/pending.rs`).
- **The monitor never allocates**: device output is `defmt` with scalar
  args; sim output goes through `println!` under `picodroid_core::hal::sim::allocator::bypass()`
  so the report cannot perturb the numbers it reports. Never build a
  `String`/`Vec` for monitor output.
- `parity::ALLOCS` (parity-metrics) stays separate from
  `GcState.alloc_total` (mem-diag): the former is atomic, sim↔device
  equality-checked, and never resets; the latter is non-atomic and drained
  per window. Do not merge them.
- Offensive-mode features and canaries shift heap_4 block sizes — leave
  them off for byte-exact parity runs (`docs/parity-audit.md`).
- New direct native alloc sites (`heap.objects.alloc(...)` outside the
  interpreter) should call `system::mem_diag::note_native_alloc(n)` under
  `#[cfg(feature = "mem-diag")]` so `nalloc` stays honest.

## Java-side counterpart

Apps and Java tests can self-report without any of the above:
`Runtime.usedMemory()`, `Runtime.peakMemory()` / `resetPeakMemory()`,
`Runtime.gcCount()` / `gcFreed()` / `gcTimeNanos()` / `resetGcStats()` —
backed by the same `live_bytes` accounting the monitor prints.

## Churn-reduction playbook (what the counters already paid for)

Measured with this monitor and fixed measure-first (reference for the next
hunt): recycled `SensorEvent` (1ac965f) and `MotionEvent` (1492d23) and
`KeyEvent` (this series) — steady-state input dispatch allocates nothing;
`intern_dyn_owned` buffer handoff + `String.format` scratch reuse killed the
copy-per-dynamic-string. The pattern: find the per-event allocation with
`nalloc`/histogram, allocate once with the full field span
(`alloc_with_field_count`), rewrite fields per event, root it in
`visit_gc_roots`, clear it in `reset_dispatch_event_state`.
