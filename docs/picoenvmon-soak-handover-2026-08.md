# picoenvmon Nightly Soak Handover — post GC-race fix (2026-08-17)

Runbook for the session that re-runs the long soak now that the corruption is
fixed. Supersedes the execution premise of
`picoenvmon-soak-plan-2026-08.md` (written while the P1 was open); that doc's
signal-triage tables (§4) and expected-noise list (§5) remain valid and are
not repeated here. Investigation history: `picoenvmon-qa.md` (2026-08-17
sections).

What changed since the original plan was written:

- The P1 corruption is **fixed** (`0c1326d`): FreeRTOS SMP equal-priority
  wake-yield interleaved compound heap operations; `pico_jvm::atomic_section`
  scheduler-suspension guards closed it. The 16-min repro (3/3 kill rate)
  validated clean.
- **Offensive mem-diag now actually arms on device.** It was silently
  sim-only before; every prior on-device "offensive" run had inert checks.
  Boot must print `memmon: offensive checks ON (build-baked)` — if that line
  is missing, the build env was wrong and the soak's trap coverage is zero.
- Every injected button press is verifiable in the RTT log (`beb0e3d`):
  `pdb: key N -> pin P` (receipt), `key: code=N action=A ...` (dispatch),
  `activity: push/pop <Class>` (screen change).
- The verified-press soak drivers are now checked in under `scripts/soak/`.

## Objectives (both were blocked on the crash; both are now runnable)

1. **Heap-gate part B**: a sustained combined-load soak (dashboard serving +
   nav churn + forced refreshes), hours long. Go-criteria: zero OOM lines,
   min-ever native free (`nmin`) ≥ 40 KB, largest block (`lblk`) ≥ 16 KB at
   matched idle after the soak, no unexplained reboot, and — new — zero
   firings of the offensive traps (span/overlap invariant, root audit,
   poison, post-GC integrity).
2. **PEM-3 prereserve retune** — see the dedicated section below.

## Build & flash

```bash
pkill -f 'probe-r[s]'   # bracket trick is mandatory; bare pattern self-kills
env $(grep -v '^#' .wifi-creds.env | xargs) \
  PICODROID_EXTRA_FEATURES=mem-diag PICODROID_MEMDIAG_OFFENSIVE=1 \
  ./scripts/flash.sh --board pico_enviro_mon_w --app picoenvmon --release \
  > /tmp/soak-rtt.log 2>&1 &
```

Operational rules (unchanged, learned the hard way): flash.sh never exits —
its background task ending IS a panic alarm; never overlap flash attempts;
never run sim.sh concurrently for the same app; disable or plan around the
3 AM sim-run / 4 AM hil-run cron (the 4 AM one pkills the probe and may
flash the attached board — comment the crontab entries with a marker and
re-enable at teardown). The RTT log grows ~10 MB/hour under load; put it
somewhere with room.

Offensive-build overhead is acceptable for soaking: the per-alloc
span/overlap sweep is O(live objects) and the measured production-build
guard cost is 0.81% (`picoenvmon-qa.md` measurement table); offensive
builds are for trapping, not for perf numbers.

## Driving load: `scripts/soak/`

| Script | Role |
|---|---|
| `soak-lib.sh` | `press` (pdb keyevent + RTT verification), `open_screen` (relative hub nav — LVGL focus WRAPS, never assume "A to top"), `resync_to_hub` (Y until no pop; Y at hub is a no-op) |
| `soak-nav.sh` | one four-screen churn cycle; args: `refresh` (Network X = forced NTP+weather), dwell seconds for Live |
| `soak-http.sh <ip>` | 2 s dashboard fetch loop + 3-way burst every ~30 min |
| `soak-watch.sh <ip>` | RTT alarm scan → `/tmp/soak-alerts.log`, panic flag on fatal signatures, probe liveness, dashboard-uptime reboot detector |
| `soak-master.sh` | phase driver: smoke 15 m → combined 60 m → quiet hold 60 m → hourly bursts until 07:00; stamps each phase with the RTT byte offset |

Env overrides: `SOAK_RTT`, `SOAK_NAV`, `SOAK_FLAG` (defaults `/tmp/soak-*`).
Start order: flash → wait for `net: up, ip` → `soak-watch` → `soak-http` →
`echo 0 > /tmp/soak-hub-selection` → `soak-master`.

Driver gotchas baked in but worth knowing:

- **Never press X on Live** — the focusable Logger Switch toggles sensor
  logging off (the drivers avoid it; don't add it).
- **Settings produces two false FAILs per cycle** (`FAIL key=23/4
  no-dispatch-log`): the NumberPicker edit-mode filter consumes X/Y natively
  before the Java queue. Benign; the cycle's closing `activity: pop` PASS is
  the real health signal. (An edit-mode log line is an open follow-up —
  `followups-2026-08.md`.)
- History X opens a row dialog; the drivers dismiss it via Y and the
  `key: BACK -> dialog dismissed` line verifies it.

## PEM-3: retuning the memory pre-allocation numbers

> **Status 2026-08-31 — the values were retuned; the on-device validation was
> not.** `a58df06` ("prereserve retune for the packed-arena era", 2026-08-18)
> set `prereserve_arena_values` 3072 → **1536** and added
> `prereserve_arena8_bytes = 3072` after C4 moved `byte[]` off the i32 arena.
> But it was derived from the **sim** memmon storage steady state, not from the
> device quiet-hold procedure in steps 1–3 and 5 below — so the zero-growth
> validation on hardware and the before/after record in `picoenvmon-qa.md`
> (step 6) are still owed. Note the anchors below are stale: the keys now live
> at `board.toml:72-77` and there are **six** of them, not five.

**What it is.** At boot the JVM pre-reserves its slot/arena storage from
`[jvm] prereserve_*` in
`platforms/rp/boards/pico_enviro_mon_w/board.toml:62-66`:

```toml
prereserve_obj_chunks    = 5      # object slots, 64-slot chunks
prereserve_arr_chunks    = 3      # array slots, 64-slot chunks
prereserve_str_chunks    = 8      # dyn-string slots, chunks
prereserve_fields_values = 2560   # object fields arena, Value slots
prereserve_arena_values  = 3072   # array data arena, i32 values
```

These were copied from the non-W board, sized **without** the network
thread. Wrong prereserves cost either boot-time waste (too big) or runtime
growth events — each growth is a FreeRTOS allocation that fragments the
heap, which is exactly what the ChunkedSlots design exists to avoid.

**Evidence they are wrong:** during the 2026-08-17 runs,
`memmon: storage` reached `arena_cap=4075` — the array arena grew past its
3072 prereserve during normal serving (four growth events). One measured
data point, but taken with a dying network thread; re-derive properly.

**Procedure:**

1. During the soak's **quiet hold** (dashboard serving, Logger on, NO nav
   input, network thread alive — verify pages are actually serving, since a
   dead serve thread invalidates the numbers, which is what voided the
   2026-08-16 attempt), collect every `memmon: storage w=.. obj_chunks=..
   arr_chunks=.. str_chunks=.. fields_cap=.. arena_cap=..` line. They print
   on growth events, so ALSO capture the last one before the quiet hold and
   the first one after — if none print during the hold, the pre-hold line IS
   the steady state.
2. Confirm the combined phase didn't push any value higher than the quiet
   hold shows (grep the whole log; take the max).
3. New value per key = **observed steady-state max + one growth step**:
   chunks keys +1 chunk; `fields_values` +256 (`FIELDS_ARENA_CHUNK`);
   `arena_values` has no fixed chunk (the array arena grows by exact
   request size, `array_heap.rs:110-118`) — give it ~+512 i32s of margin,
   which covers the largest single array the app allocates.
4. Edit the five keys in the W board.toml only (the plain
   `pico_enviro_mon` stays as-is — no network thread there). The board.toml
   parser is line-based: **no inline comments on value lines**.
5. Validate: reflash, boot, run 15 min of combined load — the goal state is
   **zero `memmon: storage` growth lines after boot settles**. Then re-run
   the sim heap gate (`sim.sh -b pico_enviro_mon_w -a picoenvmon -l 360` +
   curl loop) to confirm the larger prereserve still fits the 360 KB cap.
6. Record the before/after in `picoenvmon-qa.md` and close the PEM-3 line
   in `perf-memory-handover-2026-08.md` §5.

## Current device state (as left on 2026-08-17)

The attached `pico_enviro_mon_w` is running the validated fix build
(offensive armed, commit `0c1326d` tree) and serving. Artifacts from the
investigation live in `build/soak-2026-08-16/` (107 MB RTT log + nav/http/
alert logs with per-phase byte offsets). The nightly cron entries are
re-enabled.
