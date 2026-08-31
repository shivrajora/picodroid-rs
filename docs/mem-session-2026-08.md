# Memory-attribution session — 2026-08-17

Goal: attribute picoenvmon's heap to code constructs with the new mem-diag
**heap census** (docs/memory-diagnostics.md), then land measurement-gated
reductions. Companion to docs/perf-memory-handover-2026-08.md (§3 memmon
child-GC blindness is FIXED by this session; §6 duplicate class metadata is
MEASURED below).

Sim = host simulator with the device heap model (heap_4 arena, 416 KB
RP2350). `devB~` figures are the census's 32-bit release re-derivations —
use those for device budgeting, not the host `parsedB`.

## Tooling landed first (Phase A)

- `gc=`/`freed=` moved to heap-wide `GcState` — child-thread GCs now visible
  (they were `+0` before; confirmed live: serve-loop GCs on the
  NetworkManager thread now count). New `gcb=+N` column = bytes reclaimed.
- `heapcensus` control verb + census in every snapshot: live bytes by class,
  arrays by element type, dyn-string len/cap/buckets, side tables
  (ArrayList/HashMap/sb/lambda/exceptions), per-executor class metadata.
- `Jvm::parsed_metadata_bytes()` / `class_table_bytes()`;
  `mem_diag::register_child_jvm` registry prices each child executor live.

## Baseline (pre-optimization), commit = working tree on 6ea33d3

### picoenvmon, sim `-b pico_enviro_mon_w -m` + HISTO, 416 KB cap

Steady state serving 1 req/s after one nav cycle (w≈225, run log
`baseline-picoenvmon-w.log` in the session scratchpad):

| Metric | Value |
|---|---|
| `live` | 19,693 B (obj 3,312 + arr 15,704 + str 677); floor 19,622 flat |
| `nused` | 340,520 B (nfree 85,464; nmin 74,736; lblk 53,768; frag 371pm) |
| Serve churn | ~6 allocs + ~6 `intern_dyn` per page render; GC every few windows (idle GC + pacing), `gcb` ≈ +70..215 B/GC |
| Leaks | none (0 LEAK?, 0 GC-PRESSURE over ~4 min) |

Census highlights:

| Construct | Bytes | Note |
|---|---|---|
| **classmeta main** | 62/161 parsed, `devB~` **49,387** (+ table 3,240) | largest single JVM-attributable consumer |
| **classmeta child NetworkManager** | 11/161 parsed, `devB~` **8,735** (+ table **5,140**) | the §6 duplicate, measured: ≈13.9 KB device; child table = 5,140 B because `Jvm::new()` grows by doubling to cap 256 (main pre-sizes to 161 → 3,240 B) |
| `byte[]` arrays | 8n / **10,824 B** (0 inline) | HTTP pageBuf/reqBuf/page constants — 4 B/element cost, the C4 packed-byte target (→ ~2.9 KB packed) |
| `int[]`/`float[]` | 22n / 4,048 B | sensor rings + snapshots |
| Objects total | 44n / 3,312 B across 25 classes | tiny — objects are NOT the problem |
| dyn strings | 32n / 677 B cap | small at steady state; churn, not retention |
| side tables | 84 B | negligible for picoenvmon |

Nav cycle (SettingsActivity open): obj census peaks at 71n/8,196 B —
LinearLayout 6n/1,512 B, TextView 6n/1,416 B, NumberPicker 3n/948 B. View
objects are cheap; the LVGL cost is native-side (48 KB pool, invisible here).

### Benchmarks (3-run medians pending; single-run figures)

| Bench | testbench_rp2350 (thr 256) | pico_enviro_mon_w (thr 64) |
|---|---|---|
| perfbench SCORE | **732** (speed 640 / mem 43 / mixed 49) | **1018** (747 / 147 / 124) |
| perfbench GCs | 59 (526 µs) | 240 (1,424 µs) |
| benchmark TOTAL | 844 ms | — |

**The `gc_alloc_threshold=64` penalty is quantified: +39 % composite score,
4× GC count — on every workload, not just serving.** (Handover §2's open
measurement.)

## Correction: the orphan-sim discovery

Mid-session, `ss -ltnp` showed port 8080 held by an **8-hour-old orphaned
sim** — 22 orphans accumulated across sessions because the FreeRTOS host
sim renames its process to "Scheduler", so the documented
`pkill -x picodroid` matches nothing (memory:
`reference_sim_orphan_scheduler_pkill`). Consequences for data hygiene:

- Every picoenvmon sim this session (and possibly the 2026-08-16 gate)
  failed to bind :8080; "serve churn" in the baseline was actually
  **BindException + ServerSocket retry churn** (histogram-confirmed:
  `BindException=21 ServerSocket=21` marching in lockstep), and curls were
  answered by the stale orphan.
- The true post-fix idle signature (clean bind) is **+2 alloc / +1 stri
  per second = the 1 Hz accept-timeout `SocketTimeoutException` + message**
  — a framework-level churn source (recycled-exception candidate, NET
  follow-up). While actively serving, the accept doesn't time out and this
  vanishes.
- The prior gate's `min-ever ≥ 32 KB` figure is not comparable to this
  session's gate numbers.

## Optimizations landed (all gated)

### C1 — shared class set across executors
`Thread.start` children and bg-pool workers now execute against
`boot::shared_jvm()` (a `SharedHeapCell`-style static published by
`run_app` after loading; `Jvm::invoke_*` relaxed to `&self`) instead of
building a private `Jvm` + re-running `load_classes`.
**Measured: the NetworkManager child duplicate was 8,735 B parsed +
5,140 B doubling-overshot class table ≈ 13.9 KB device-estimate — now 0**
(census shows no child rows; total device metadata at the same phase
51,131 → 41,002 B). threaddemo (child ticks) + executordemo (bg pool) pass.

### C2 — allocation-free dashboard serve path
`HttpServer.buildPage` writes the whole page (rows, footer clock/IP/uptime)
straight into `pageBuf` via byte-append helpers; `Formatter` gained
int-returning `tempCenti`/`centi`; `NetworkManager` caches weather + IP as
bytes at their refresh points; `statusFooter()`/`uptime()` deleted.
**Gate met: `stri=+0` during serving; ~1 alloc/request (the accept()
`Socket` — API-inherent).** Trade: +25 small static `byte[]` constants
(~1.6 KB), largely absorbed by C4 inlining.

### gc_alloc_threshold 64 → 128 (W board)
Three-way `-l 360` + 100-curl + nav experiment (same code): min-ever-free
10.9 / 9.9 / 7.6 KB at 64 / 128 / 256, all OOM-free — the nav dip is
Activity/LVGL/TCP transients, NOT paced-GC garbage, so 128 buys half of
64's GC frequency for ~1 KB of worst-case headroom.
**perfbench (W board): SCORE 1018 → 792 (−22 %), GCs 240 → 119, GC time
1,424 → 768 µs** — 79 % of the threshold penalty recovered.

### C4 — packed byte[]/boolean[] (1 B/element)
New `ArrayData::Inline8`/`Arena8` + `ArrayHeap::arena8: Vec<u8>`; 40 B slot
assert unchanged; `bastore` already truncated to i8 so semantics are
identical; GC compaction second pass; offensive poison/integrity cover
arena8; `data_slice` stays i32-only (GC ref-scan). 6 new unit tests; full
suite 529/529. Offensive nav+serve soak: zero violations.
**picoenvmon byte[] payload 12,428 → 3,921 B (−68 %); 27/33 byte arrays now
inline; `arr` live 17,308 → 8,801 B; total live 21.4 → 13.8 KB.**
Follow-up: `char[]`/`short[]` at 2 B/elem. **Closed 2026-08-25 — rejected.**
Built in full, then measured at **+952 B flash / 0 B heap saved** and reverted;
see `perf-campaign-2026-08.md` §S5. Root cause: picodroid `String` is byte-backed
ASCII, so `toCharArray()` is the only `char[]` source and the corpus's sole
instance is 3 elements — inline under both layouts.

### C3 — prereserve retune (W board)
Storage steady state never grew past boot (obj 5 / arr 3 / str 8 chunks,
fields 2560 kept). After C4 the i32 arena demand fell to ~920 slots:
`prereserve_arena_values` 3072 → 1536, new `prereserve_arena8_bytes` =
3072 (observed demand 2,777 B). New tunable plumbed through
`build_support` + board.toml schema + jvm-tunables.md; `[memmon] storage`
gained `arena8_cap`.

## Final `-l 360` gate (full stack, threshold 128)

| Metric | pre-C4 @128 | pre-C4 @64 | **final stack @128** |
|---|---|---|---|
| min-ever-free | 9,904 | 10,912 | **13,720** |
| largest block | 7,168 | 7,728 | **17,952** |
| live steady | ~22.1 K | ~22.6 K | **~16.0 K** |
| OOM / LEAK | 0 / 0 | 0 / 0 | **0 / 0** |

## Remaining follow-ups

- 1 Hz `SocketTimeoutException` churn on the serve thread (recycled
  exception or native accept-timeout signalling — framework change).
  Confirmed on device: the same +2 alloc/+1 stri per idle second.
- Device HTTP close sends RST after the body (curl exit 56 despite
  `http=200` and full payload) — FreeRTOS+TCP close semantics, NET
  follow-up. Scripts probing the device dashboard must judge success by
  `%{http_code}`, not curl's exit code.
- ~~`char[]`/`short[]` packing (2 B/elem) on the C4 substrate.~~ **Not open** —
  built, measured at zero, rejected (`69c918f`). `perf-campaign-2026-08.md` §S5
  records the reopen trigger: an `arena16_cap` that stops being zero.
- Device-side census over pdb sysmon (roadmap `mem-diag-histo`; census is
  the sim half). Byte-weighted GC pacing: `gcb=` now provides the evidence
  base.
- Dyn-string content dedup: census shows only ~37 live dyn strings at
  steady state — LOW value, deprioritized.
- `pico_enviro_mon` (non-W) prereserve retune after its own measurement.
- P1 (child-thread InvalidReference → GC thrash) was fixed by a concurrent
  session (`0c1326d`, SMP wake-yield atomic_section work) while this one
  ran; this session's changes were validated on the merged tree.

## Device spot-check — RUN 2026-08-18 (board freed by power cycle): PASSED

<15-min mem-diag release flash on pico_enviro_mon_w (5a72a60 firmware,
plain mem-diag, not offensive): boot + WiFi join, one full nav cycle via
prebuilt `pdb input keyevent` (all four screens pushed/popped cleanly, no
thread failures — the fixed-P1 recipe's window passed without symptoms),
240-request serve loop, detach at ~8 min.

**The sim model tracks hardware almost exactly:**

| Metric | sim (416 KB model) | device |
|---|---|---|
| `arr` live | 8,801 B | **8,801 B — byte-identical** (pointer-free layout) |
| `live` steady | ~13.8 K | ~13.0–13.7 K |
| idle churn | +2 alloc / +1 stri per window | **same** (the accept-timeout exception) |
| serve churn | `stri` ≈ +0 | **+1/window** at ~3 req/s — allocation-free serve holds |
| `nused` | ~297.9 K | 251.7–272 K — Δ ≈ 46 K matches the census's host-vs-device metadata inflation (`parsedB` 86.6 K vs `devB~` 46.3 K), **validating the `devB~` estimate on hardware** |
| min-ever free | — | **146.2 K** (lblk 147 K, frag ≤ 32 ‰) |

Child-thread GCs are visible in `gc=/freed=/gcb=` on device (the A1 fix,
proven on hardware: serve-thread collections show `freed`/`gcb` deltas).
Two `LEAK? native floor` warnings fired once each at WiFi join (+6.6 K)
and the nav cycle (+8.4 K) — legitimate construction growth, `nused`
settles flat after; sim runs don't show these because the host net stack
joins for free.

One pre-existing quirk surfaced: the device closes HTTP connections with
**RST instead of FIN** after the full page is sent (curl exit 56 with
`http=200 bytes=761` — all data delivered; browsers tolerate it).
FreeRTOS+TCP close semantics, unrelated to this session — NET follow-up.

## Final validation (2026-08-18, merged tree incl. the concurrent
## atomic-section fix 0c1326d)

- Sim smoke: helloworld / benchmark (TOTAL 833 ms, was 844) / blinky — pass.
- `./scripts/pre-commit` — **All checks passed** (clippy all boards, flash
  gates, langsuite, all tests incl. 6 new packed-array tests).
- `test-memdiag.sh` 5/5, `test-heap.sh` 5/5.
- perfbench default board: memory subscore 43 / mixed 48 — **identical to
  baseline** (43/49). The speed subscore rose 640 → ~725-756 across three
  runs: that is 0c1326d's atomic-section cost on the host sim (measured
  0.81 % on device by that session), not this session's changes — which
  also means the W-board threshold win (1018 → 792) is *understated*, since
  the 792 carries the same overhead the 1018 didn't.

## Progress log

- Phase A landed + validated (`test-memdiag.sh` 5/5; census exercised on
  helloworld + picoenvmon; child registry proven on the NetworkManager
  thread — registry retained though empty post-C1, it prices any future
  private-Jvm executor).
- Baselines captured; orphan discovery + cleanup (22 processes).
- C1, C2, threshold 128, C4, C3: landed and gated (above).
- Concurrent session landed the P1 fix (f712dec..6140c06) mid-session;
  everything above validated on the merged tree.
