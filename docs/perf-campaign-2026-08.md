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
