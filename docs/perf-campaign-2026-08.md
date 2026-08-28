# Perf & memory campaign — session log (2026-08-24)

Running log for the campaign described in the plan "measure, then climb".
Companion docs: `perf-memory-handover-2026-08.md` (the open items this campaign
picks up), `mem-session-2026-08.md` (the C1–C4 wins it builds on),
`parity-audit.md` §6 (what the sim will not tell you).

---

## S0 — Harness

Built the read side of the nightly corpus, which had been accumulating since
June and had never been parsed.

### New tooling

| File | Purpose |
|---|---|
| `scripts/bench-backfill.py` | Parses `build/{sim,hil}/logs/<RUN_ID>/*.log` into `bench/parity/history.csv`. Idempotent (rows keyed on everything but the value), so re-running replaces rather than duplicates. `--run-dir` mode for the crons to append a single fresh run. |
| `scripts/bench-report.py` | `--trend` / `--compare` / `--fit` / `--holdout` over the CSV. |

### Schema v2

```text
utc,commit,env,board,app,mode,split,metric,value
```

`board` and `mode` were added before rows landed — without `mode` the
shrink-vs-no-shrink comparison is unrepresentable, and that comparison turned
out to matter (S1 below). `split` is `train` (`benchmark`, `perfbench`) or
`holdout` (everything else); held-out apps may veto a change but never justify
one. The six legacy rows migrate automatically on first run.

### Result

**6 rows → 59,077 rows**, 46 distinct metrics, 220 sim + 156 HIL run
directories, back to 2026-04-15. 5.8 MB raw, 278 KB gzipped.

Per `(app, mode)` the parser recovers ~38 metrics where the old `parse_log`
recovered one:

- all 10 `benchmark` microbench splits, both framings (`[Benchmark] x: N ms`
  and defmt `[INFO ] Benchmark: x: N ms (...)`)
- `TOTAL: N us` — `gcstress` and `heapstress` have been emitting this on both
  lanes since June and `parity-bench.sh` never matched the `us` unit
- sim `[sim] JVM wall-clock:` → `gc_count`, `gc_freed`, `gc_us`,
  `classes_parsed`/`classes_total` (lazy-load effectiveness, previously
  untrended)
- sim `heap phase:` lines → per-phase `cur`/`peak` bytes
- sim `OOM:` lines → count, min-ever-free, min largest-block
- `apk_bytes`, HIL flash wall time
- `parity:` counters and `[memmon]` windows are parsed but **absent from the
  backfill**: the crons enable neither `parity-metrics` nor `mem-diag`. Those
  arrive only on forward-looking `parity-bench.sh` runs.

### `scripts/parity-bench.sh` — two bugs fixed, three modes added

Both bugs would have corrupted the campaign's own data:

- `UTC` was computed once at script start. Since the CSV primary key is every
  column but the value, an N-run mode would have collapsed N samples into one
  row. Now re-stamped per emitted run (`stamp_utc`).
- `--app` assigned to the array rather than appending, so passing it twice
  silently kept only the last. Now appends; `--apps a,b,c` added.

Also: schema v2 emission, `TOTAL: N us` capture (previously dropped entirely),
`SCORE` no longer masked by the `wall_ms` fallback, and:

| flag | purpose |
|---|---|
| `--runs N` | N samples emitted as N rows, never pre-averaged. Sim defaults to 1 (counters are deterministic); HIL defaults to 3, **all against one flashed image**. |
| `--size-only --boards a,b` | flash/RAM per board via `arm-none-eabi-size`, plus headroom. |
| `report_spread` | flags a same-image HIL batch whose p2p exceeds 0.5% as a bad board state rather than a code change. |

### Size-lane baseline (verified 2026-08-24, commit `21e41a6`)

```text
testbench_rp2040   flash  881,827 /   917,248  (96%,    35,421 free)
                   ram    227,704 /   262,144  (86%,    34,440 free)
testbench_rp2350   flash  896,215 / 2,883,584  (31%, 1,987,369 free)
                   ram    522,852 /   532,480  (98%,     9,628 free)
```

45 s for both boards. This is the campaign's primary instrument for the
flash-first phase: zero noise, and the RP2040 program region is the binding
budget at 96% full.

**Where the RP2350 RAM actually goes** (`arm-none-eabi-nm`, release): `ucHeap`
425,984 + LVGL `work_mem_int` 65,536 + `BAND_BUF` 12,800 = 96% of BSS. The
"98% full" figure is three deliberately-sized buffers, not code. JVM runtime
structures live inside `ucHeap`, which had 146 KB min-ever-free on device for
picoenvmon — so the planned speed fixes are **not RAM-blocked on RP2350**.
Flash is the wall, and `.text` splits lvgl 218,184 / pico_jvm 134,396 /
picodroid_core 106,282 / core(Rust) 53,716.

---

## S1 — Forensics

Three open questions resolved from the backfill alone. **Zero device time
spent.** All three had been open in `perf-memory-handover-2026-08.md` or
`followups-2026-08.md`.

### 1. The `a828229` +4% is NOT a regression — verdict: layout

`docs/followups-2026-08.md` §7 and the campaign plan flagged a 4.0% device
step (`edd1259` 168,022 ms → `c5f5799` 174,690 ms, five nights 32 ppm apart),
with `a828229` ("route GC compaction and touch median off the generic sort")
the only code commit in the range.

Every deterministic sim counter is **byte-identical across the range**:

| metric | `edd1259` | `c5f5799` |
|---|---|---|
| `gc_count` | 403 | 403 |
| `gc_freed` | 100,062 | 100,062 |
| `heap_peak_kb` | 277 | 277 |
| `oom_count` | 50 | 50 |
| `classes_parsed` | 8 | 8 |
| `apk_bytes` | 5,733 | 5,733 |

The GC ran the same number of times and freed the same bytes. The JVM performs
identical work; the same work simply executed more slowly.

The per-microbench split corroborates it — the deltas scatter in **both
directions**, with the largest movements in workloads `a828229` cannot causally
touch (it changed array-arena compaction, GC compaction, and the XPT2046 median
filter):

| microbench | delta | can `a828229` reach it? |
|---|---|---|
| `control_flow` | **+39.5%** | no — branch loop, no arrays, no GC |
| `long_arithmetic` | +28.2% | no — integer math |
| `float_arithmetic` | **−18.4%** | no |
| `interface_dispatch` | +11.8% | no |
| `array_operations` | −0.03% | yes — and it did not move |

**Conclusion: `a828229` is exonerated.** Do not revert it. The plan's S1 item 2
is closed and the F3 "residual quicksort" item loses its regression pairing
(it stands on flash alone).

### 2. The "6.8% shrink tax" does not exist — verdict: layout, mean ≈ 0

`perf-memory-handover-2026-08.md` recorded shrink builds as consistently ~6.8%
slower on device, a stable unexplained standing tax on the configuration we
ship. Across all 29 paired nightly observations it **flips sign**:

| period | shrink vs no-shrink |
|---|---|
| 2026-07-27 → 07-31 | −9.8% … −2.8% (shrink **faster**) |
| 2026-08-01 → 08-14 | −6.6% … −4.0% (shrink **faster**) |
| 2026-08-15 → 08-24 | +4.3% … +8.0% (shrink **slower**) |

n=29, mean **−1.71%**, median −4.01%, stdev 5.63%, range −9.79% … +7.95%.

**Conclusion: there is no shrink tax.** The 6.8% figure came from sampling only
recent runs. `followups-2026-08.md`'s framing of this as a real cost, and the
plan's hope that F1 (native dispatch table) would recover 6.8%, are both
retired. F1 remains justified on flash (~18 KB of string-match dispatch) but
must not be sold as a speed win.

### 3. Layout noise is far larger than documented — σ ≈ 4%, ±40% per microbench

`docs/picoenvmon-qa.md` estimated the XIP/icache layout band at ±5% from an n=3
A/B/C table. The 29 shrink pairs are 29 independent samples of *two
differently-laid-out binaries doing identical work*: stdev of the difference is
5.63%, so **σ ≈ 5.63/√2 ≈ 4.0% per binary** (±8% at 2σ). Caveat: shrink is not
a pure layout change, so this is an upper bound that includes any real shrink
effect — but since the mean is ≈0 and the sign flips, the variance is
layout-dominated.

Per *microbenchmark* the band reaches **±40%** (S1.1 above). The aggregate
±5% is the result of ±40% per-workload swings partially cancelling.

Consequences, now encoded in `bench-report.py`:

- Per-test device timings are classified `attribution` — they localise a
  change, they never adjudicate one.
- Aggregate device `wall_ms` gates at 2% warn / 5% fail **across rebuilds**,
  and only at 0.7% / 2% for same-flashed-image batches (`--same-image`).
- The planned 75-minute device layout sweep (plan S1 item 4) is **no longer
  needed for calibration** — reserve the `layout-jitter` mechanism for
  arbitrating a specific contested change.

**The methodological trap worth remembering:** same-image reproducibility is
32 ppm, so a cross-image comparison looks *extremely* precise. `dcb377c`
produced −6.56% ± 0.01% eight nights running. That is eight confirmations of a
conclusion that is wrong. Precision is not accuracy; only the long series
exposed it.

### 4. The sim→device proxy is not identifiable from this corpus

`bench-report.py --fit` returns strong *negative* correlations (sim `wall_ms`
r = −0.77, `heap_peak_kb` r = −0.94, n=11). That is a temporal confound, not a
relationship: over the backfilled window the sim got **15.8% slower**
(565 → 654 ms; largely `0c1326d`'s atomic sections, which cost more on the host
than on device) while the device got **28.4% faster** (243,993 → 174,695 ms).

The tool now prints this caveat inline so the numbers cannot be misread later.
Fitting a usable proxy needs paired runs with `parity-metrics` enabled
(`parity-bench.sh --both`) — collect forward, do not mine backward.

---

## Consequences for the remaining plan

- **S1 is complete**, ahead of schedule and at zero device cost. Items 2, 3
  and 4 are all closed; item 1 is closed as *not answerable from this data*.
- **No regression to fix.** The campaign's opening 0–4% is not there. Both
  suspected defects were measurement artifacts — which is itself the strongest
  possible argument for having built the harness first.
- **F1 is reweighted**: flash-only justification (~18 KB), no speed claim.
- **F3 decoupled** from the `a828229` investigation; still worth doing on flash.
- **Device wall-clock is a coarser instrument than assumed.** With σ ≈ 4%, the
  plan's batching rule tightens: accumulate changes until the predicted effect
  exceeds ~8% (2σ) before spending a device confirmation, not 5%.
- The deterministic metrics are proven load-bearing: they answered both
  questions that four months of device wall-clock could not.

---

## S2 — Flash headroom

Objective for this phase: buy program-region headroom on the RP2040, which is
the campaign's binding budget (96.1% full, ~35 KB, enforced only by link
failure in `scripts/pre-commit`). Every later speed change adds code and has to
pass that gate.

Both changes were measured with `parity-bench.sh --size-only`, which is the
only campaign instrument with a literally zero noise floor.

### F3 — hand-written introsort (`jvm/src/sort.rs`)

`edd1259` and `a828229` had already funnelled every primitive `Arrays.sort`
and GC compaction onto one `u64` key. What remained was the standard library's
driftsort/ipnsort — 7,386 bytes across five symbols even at a single
instantiation. The JVM's two sort callers are `Arrays.sort` on app-sized
arrays and arena compaction (once per collection, 403 times in a whole
`benchmark` run); neither is bound by sort throughput.

Replaced with insertion sort under 16, median-of-three **Hoare** quicksort,
and a heapsort fallback at `2*log2(n)` depth. Hoare rather than Lomuto so an
all-equal run splits down the middle instead of going quadratic — `Arrays.sort`
on a constant array is ordinary. The depth limit is what keeps compaction off a
latency cliff.

```text
sort machinery   10,074 ->  1,774 bytes   (9 -> 5 symbols)
rp2040 flash    881,827 -> 871,939        (-9,888)
```

Byte-identical work: `gc_count` 403, `gc_freed` 100,062, `heap_peak_kb` 277,
`oom_count` 50, `classes_parsed` 8 — all equal to the `2332f36` nightly
baseline; `insns` 65,965,597 against the parity audit's 65,965,594.

### F2 — premultiplied ARGB8888 was dead code, and on by default

`LV_DRAW_SW_SUPPORT_ARGB8888_PREMULTIPLIED` was never set in `lv_conf.h`.
LVGL's `lv_conf_internal.h` defaults it to **1** when `LV_KCONFIG_PRESENT` is
undefined, so it has been linked since the port began without anyone asking
for it.

It is reachable exactly two ways, and both are closed:

- **An asset in that format.** `tools/papk-pack` bakes
  `LV_COLOR_FORMAT_RGB565` into every image and has a `color_format_guard`
  test module enforcing it.
- **An intermediate layer requesting it.** Every `lv_draw_layer_create` call
  site in `vendor/lvgl` passes `ARGB8888`, `A8`, or `NATIVE`. None passes
  `PREMULTIPLIED`.

```text
rp2040 flash    871,939 -> 849,047        (-22,892)
rp2350 flash    886,919 -> 868,943        (-17,976)
```

The win exceeds the two obvious symbols (`blend_image_to_` 8,234 +
`blend_color_to_` 3,058 = 11,292) because the config also drops the
premultiplied transform and recolor paths.

**Proof of no behavioural change:** graphicsbench built with `parity-fbhash`
before and after emits **1000 band CRC32s that are identical**, with
`insns=147899 allocs=8908 gcs=34 bands=1000 fbytes=9648348` equal in both runs.
Rendering is pixel-exact. `imagedemo` (asset decode), `animdemo` (alpha
animation — the path that *does* create ARGB8888 layers) and `dialogdemo` all
hit their `hil-tests.conf` patterns.

Two neighbours were checked and left alone: `lv_draw_sw_transform` (9,782 B) is
live via `ImageView.setScale`, and plain `LV_DRAW_SW_SUPPORT_ARGB8888` is
genuinely needed — the animation engine's `lv_obj_set_style_opa` makes LVGL
build ARGB8888 intermediate layers.

### S2 result

```text
rp2040 program region   881,827 -> 849,047 bytes
headroom                 35,421 ->  68,201 bytes   (+92%)
region occupancy          96.1% ->    92.6%
```

### Correction to the plan's F1

The plan sized F1 (native dispatch → sorted table) at ~18 KB from the symbol
sizes of `pico_jvm::native::string::dispatch` (10,316) and
`PicodroidNativeHandler::dispatch` (7,588). Reading them, both are `match`
statements whose arms contain the **method implementations**, not just
comparison code. Most of those bytes are real work and would survive any
dispatch rewrite. F1 needs a measured prototype before it can be scoped; the
18 KB figure should not be quoted.

S1.3 already removed its other justification — there is no 6.8% shrink tax for
a faster dispatch to recover.

---

## S4 — Interpreter overhead, and the limit of what we can measure

Four changes, each removing work from a hot path without changing what the JVM
computes. Every deterministic counter is identical before and after — `insns`
65,965,597, `allocs` 100,263, `gcs` 403, `gc_freed` 100,062, `heap_peak_kb`
277, `oom_count` 50 — which is the expected signature of pure overhead
removal, and also the reason the deterministic proxy cannot score them.

| | change | flash |
|---|---|---|
| S1 | one-entry `(class_idx, method_idx) -> code` cache in the interpreter loop, replacing a OnceCell load + `Box` deref + two Vec indexes + a reslice **per bytecode** | } +100 B |
| S2 | `has_lambdas()` gate so a virtual invoke skips the lambda probe entirely when no proxy exists | } together |
| S3 | one `AtomicSection` per `alloc_with_defaults` instead of one **per field written** — on device each was a `vTaskSuspendAll`/`xTaskResumeAll` pair | +364 B |
| S4 | binary-search `lookupswitch` (JVMS 6.5 guarantees sorted keys) | +128 B |

S3 is safe because the object is not published: `idx` is still local to the
call, so no other task can read or write those slots, and the capacity was
sized exactly above so the lazy-grow path is unreachable. One section still
brackets the run because another task could be reallocating `fields_arena`.

### The measurement failed, and that is the finding

| environment | result |
|---|---|
| sim, 3 runs, p2p 0.8% | **−7.09%** (649 → 603 ms) |
| device, 3 runs, p2p 0.004% | **+0.42%** (184,427 → 185,201 ms) |

Both batches are internally clean — the device pair is 40 ppm, an order of
magnitude inside the 0.5% bad-batch guard. The disagreement is not measurement
error.

The per-microbench breakdown settles which one to believe, using the same
diagnostic that exonerated `a828229` in S1.1:

| microbench | device | sim predicted |
|---|---|---|
| `float_arithmetic` | **−32.0%** | −7.9% |
| `string_operations` | −11.6% | +2.9% |
| `int_arithmetic` | −10.0% | −14.2% |
| `control_flow` | +8.2% | −6.5% |
| `object_allocation` | +7.0% | +1.3% |
| `interface_dispatch` | +11.2% | +2.9% |
| `method_dispatch` | **+11.3%** | −2.2% |

Scattered in both directions, magnitudes far beyond any mechanism these four
changes could produce, and uncorrelated with the sim. That is the layout
signature. **The device cannot resolve an effect this small**: the true value
is somewhere inside ±4% and this experiment does not narrow it.

Two consequences:

1. **The plan's batching rule does not work.** It said to accumulate changes
   until the predicted effect exceeds 2σ, then spend one device confirmation.
   The predictor is the sim, and the sim is not merely noisy here — it is
   **biased**, over-predicting by about seven points. A host CPU with deep
   out-of-order execution and large caches rewards removing pointer chasing far
   more than a Cortex-M33 executing from XIP flash does. A rule keyed to an
   unreliable predictor is not a rule.
2. **Resolving this class of change needs the layout sweep**, not a bigger
   batch. n=7 per arm at ~6 min per sample resolves roughly 3%; nothing cheaper
   works, because layout noise is multiplicative over the whole run and no
   choice of workload escapes it.

### What was kept, and on what grounds

All four, at +592 B against 67,609 B of headroom — **but not as a measured
win, and the sim's −7% is not a claim.** Each is kept because it is
mechanically strictly less work, which is a different and weaker justification
than the campaign normally accepts, and it is recorded as such:

- **S1** removes memory traffic on the single hottest path in the system, for
  about 50 bytes. The best mechanism-to-cost ratio of the four.
- **S2** likewise, on every virtual invoke, for about 50 bytes.
- **S3** deletes real FreeRTOS calls and shortens the worst-case
  scheduler-suspension window — correctness-adjacent given this project's SMP
  history (`project_picoenvmon_soak_child_thread_gc_death`).
- **S4** is the weakest: `benchmark` contains no sparse switch, so this
  experiment could never have scored it. Kept as cheap insurance against a
  latency cliff in an app we have not seen, the same reasoning that chose Hoare
  partition over Lomuto in F3.

Reverting strictly-less-work changes because the instrument is too coarse would
be as much a mistake as claiming a win from a biased proxy. The honest record
is that we do not know their device effect, only that it is under about 4%.

---

## S5 — `char[]`/`short[]` packing: built, measured, REJECTED

C4's follow-up, listed in `mem-session-2026-08.md` and in this campaign's plan
as the next memory win: extend the packed-array substrate with
`Inline16`/`Arena16` at 2 bytes per element, backed by a new
`ArrayHeap::arena16`.

It was implemented in full — variants, alloc, load/store, length, `live_bytes`,
the atype census, offensive poison (`POISON_U16`), the integrity sweep's third
span space, `data_slice` exclusion, a third compaction pass, the
`memmon: storage` line, and six tests including a compaction survival test
that interleaves all three arenas. 1,962 tests passed.

The semantics work out cleanly, which is worth recording because it is the
non-obvious part. `char` is unsigned and `short` is signed, and that is the one
place the two packed types differ. The opcodes already narrow on both sides
(`castore`/`sastore` truncate to 16 bits; `caload`/`saload` re-apply their own
extension), so it is enough for `load` to widen per `atype` — zero-extend for
`char`, sign-extend for `short` — and every reader, bytecode or native, sees
exactly what it saw when these arrays were i32-backed.

**Then it was measured, and it buys nothing.**

| | |
|---|---|
| flash cost | **+952 B** (rp2040 849,639 → 850,591) |
| heap saved, every app in the tree | **0 B** |

The live-set census after a full `langsuite` run — which includes `stringdemo`,
the only example that touches `char[]` — is `ref=6n/240B(inl 6) int=1n/40B(inl
1)`. No live `char[]` or `short[]` at all. The single `toCharArray()` in the
corpus is `"abc".toCharArray()`: three elements, which is inline under both the
old i32 form (≤ 8 slots) and the new packed form (≤ 16 elements), so not one
arena byte moves either way.

Nothing else allocates them. picodroid's `String` is byte-backed ASCII, so
`toCharArray` is the only source of `char[]`; the sensor rings are `int[]` and
`float[]`; the HTTP buffers that made C4 worth 12.4 KB are `byte[]`. The SDK
exposes `Arrays.sort(char[])` and `Arrays.fill(short[])`, so an app *could*
allocate a large one — but none does, and "an app might" does not buy 952 bytes
of the budget this session exists to protect.

**Reverted.** The working implementation is kept at
`scratchpad/c5-rejected/array_heap.rs` for whoever revisits it; the substrate
and the sign-extension design are the hard parts and they are solved.

Reopen it when a workload actually allocates a `char[]`/`short[]` larger than
16 elements. The census makes that easy to notice: an `arena16_cap` that stops
being zero is the trigger.

### Why this is the session's most useful result

S4 kept four changes on mechanism because the instrument was too coarse to
score them. S5 is the opposite case and the reason that was not simply
rationalisation: here the instrument *was* sharp enough — the heap model is
byte-identical to hardware — it returned zero, and the change was reverted
despite being complete, correct, tested, and on the plan.

A campaign that only ever ratchets forward is not measuring anything.

---

## Conclusion — 2026-08-25

### What the campaign actually bought

| | before | after |
|---|---|---|
| rp2040 program region | 881,827 B (96.1%) | **849,639 B (92.6%)** |
| rp2040 headroom | 35,421 B | **67,609 B** (+91%) |
| rp2350 flash | 896,215 B | 869,551 B |
| rp2350 static RAM | 522,852 B | unchanged |
| device wall-clock | — | **no measurable change** |

Both flash wins were dead code that nobody had looked for: a standard-library
sort the JVM did not need, and an LVGL pixel format that was on because
`lv_conf_internal.h` defaults it on. Neither was on the plan. Both came out of
the harness built in S0.

The speed work produced no measurable device change, and the memory work was
reverted. That is the honest ledger.

### Standing state

- `bench/parity/history.csv` — 59k+ rows back to 2026-04-15, appended by both
  nightlies automatically, with a drift summary in the emailed report.
- `bench/parity/ratchet.toml` — flash/RAM baseline, gated at 0% in both
  `scripts/pre-commit` and the CI `size-ratchet` job.
- `scripts/bench-report.py` — `--trend`, `--compare`, `--ratchet`, `--holdout`,
  `--fit`.
- `parity-bench.sh --check` — size ratchet plus sim↔device counter parity. The
  old 30% hil/sim ratio alarm is gone; it was ~10,000× the device noise floor,
  never fired in four months, and the ratio is not a meaningful quantity now
  that the sim is known to be a *biased* predictor rather than a noisy one.
- 4AM `hil-run.sh` cron **re-enabled 2026-08-25** (disabled 2026-08-24 for the
  campaign). The one-day gap in the HIL trend is this, not a failure.

### The three things worth carrying forward

1. **Deterministic metrics decide; wall-clock confirms.** Every conclusion in
   this campaign came from a counter, a byte count, or a CRC. Every wall-clock
   number that looked like a finding was layout.
2. **The sim is biased, not just noisy.** It over-predicted a device speed
   change by seven points (S4). Never size a device change from a sim
   wall-clock delta.
3. **Same-image precision is not accuracy.** 32 ppm reproducibility makes a
   cross-image comparison look authoritative. `dcb377c` reported −6.56% ± 0.01%
   eight nights running, and it was wrong (S1.2).

### What is left, and why it was not done

- **Device speed below ~4% is not currently measurable.** The layout-jitter
  sweep (n=7/arm, ~84 min unattended, resolves ~3%) is the prerequisite for any
  further speed work, not an optional refinement. Nothing else is cheaper:
  layout noise is multiplicative over the whole run, so no choice of workload
  escapes it.
- **F1 (native dispatch → sorted table)** was sized at ~18 KB in the plan from
  the dispatch symbol sizes. Reading them, most of those bytes are method
  *bodies*, not comparison code, and S1.2 removed the other justification by
  showing there is no shrink tax to recover. Needs a measured prototype before
  anyone quotes a number.
- **F4 (framework class tree-shaking)** is real but a large change, and flash
  is no longer tight enough to justify it.
- **`Value` 16 B → 8 B** remains the largest memory lever in the codebase and
  still deserves its own design doc. It is a redesign with a single
  cliff-shaped payoff, not a hill to climb.

---

## Post-campaign — the nightly append log

The S0 wiring made both nightlies append to `bench/parity/history.csv`, which
is tracked. Two nights later that showed its cost: the working tree is dirty
every morning, and the reflex response to a dirty *generated* file —
`git checkout -- .` — would silently destroy rows nothing can rebuild.

Measured first, because the obvious worry turned out to be the wrong one:

| | raw | stored in pack |
|---|---|---|
| current CSV | 6.00 MB | 298,882 B (base) |
| previous version | 5.91 MB | **273 B** (delta) |

Append-only text deltas almost perfectly, so a nightly commit costs a few
hundred bytes, not 6 MB. File size was never the problem. Data loss was.

### The file is now regenerable

Of 61,676 rows, 61,072 rebuilt from `build/*/logs`. The 604 that did not were
the ones written straight to the CSV by lanes that leave no log: the size lane
(`--size-only`) and any ad-hoc `parity-bench.sh` run whose `mktemp -d` log
directory has since been cleaned up.

The size lane now writes `build/size/logs/<RUN_ID>/<board>.size.log` and goes
through `bench-backfill.py` like every other lane. Because several boards share
one run directory, the filename cannot encode the identity, so the log declares
it on the first line:

```text
#bench board=testbench_rp2040 app=helloworld mode=no-shrink
   text    data     bss     dec     hex  filename
 849635       4  227700 1077339  10705b  .../picodroid
#program_flash_max=917248
#ram_max=262144
```

`bench-backfill.py` honours that header for any log, and derives headroom from
the recorded ceilings so no reader has to re-parse a linker script. Verified by
deleting the new size rows and rebuilding them from the logs alone.

The 189 size rows written before this change still have no log behind them.
They are the only irreplaceable rows in the file, which is why they were
committed rather than left dirty.

### The nightlies commit it

Both runners now commit the CSV after appending. Three details that matter:

- **Pathspec commit, not `git add`** — commits only `bench/parity/history.csv`
  from the working tree, so nothing a human left staged gets swept in. Verified
  against a tree that also had another session's edits to `.gitignore`,
  `scripts/sim.sh`, and an untracked `tools/kotlin-survey/`: the commit
  contained exactly one file.
- **`--no-verify`** — the hook is the 15-minute suite; this is a data append.
- **Never pushes.** The cron's own `git pull --ff-only` stays happy with
  unpushed local commits unless the remote has diverged, and pushing is a
  decision for a human.

### Committing must not touch the metric log at all

The first version of the above got this wrong in a way worth recording,
because the wrong fix looked reasonable.

`scripts/pre-commit` *is* the git hook, and two of its stages were writing the
CSV on every single commit: the size-ratchet stage (~14 rows) and the langsuite
stage, which drives `sim-run.sh` (~100 rows). The first attempt at a fix only
suppressed the resulting *commit* — which traded an unwanted commit for a
permanently dirty tree, and left the writes in place. It solved the symptom
and made the actual complaint worse.

The real problem was conceptual: the ratchet was being routed through an
append-only history to answer a question about *right now*. Those rows are, by
construction, the same numbers as the last commit's unless the image changed.
They were never worth persisting.

So the ratchet no longer touches the CSV. `parity-bench.sh --size-only` writes
its logs to a directory the caller can name (`PICODROID_SIZE_RUN_DIR`), and
`bench-report.py --ratchet --sizes-from <dir>` reads them straight back.
pre-commit points both at an `mktemp -d` it deletes on exit.

One switch, `PICODROID_BENCH_RECORD=0`, means *measure but record nothing* —
it suppresses the CSV append and the commit together, in both runners and in
`parity-bench.sh`. pre-commit sets it for both stages. It replaces the earlier
`PICODROID_BENCH_AUTOCOMMIT`, which only ever addressed half the problem.

Verified: a full `./scripts/pre-commit` leaves `HEAD` unchanged and the CSV
**byte-identical** (same sha1 before and after), and still ends
"All checks passed".

The file now changes only when something deliberately records into it: the two
nightlies, or a hand-run `parity-bench.sh`.
