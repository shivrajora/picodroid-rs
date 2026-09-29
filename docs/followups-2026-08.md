# Open Follow-ups — post GC-race fix (2026-08-17)

**Status: items 1, 2, 4, 5 and 6 closed; item 3 (log the edit-mode key drop) is open; item 7
(GC-pacing measurements) is partly open; §8's hardware tickets are unfiled.** (Corrected
2026-09-16: the line used to say items 1-6, but item 3 was never done — `events.rs` still has no
log line on the edit-mode consumption path, checked at `09e7a8b3`.)

Everything left open after the picoenvmon soak/corruption investigation
(`picoenvmon-qa.md` 2026-08-17 sections; fix `0c1326d`), other than the
nightly soak + PEM-3 retune, which have their own runbook:
`picoenvmon-soak-handover-2026-08.md`. Ordered by risk.

Completed items: [completed/followups-2026-08.md](completed/followups-2026-08.md) — §1 (sim `Thread.start` parallelism), §2 (`sb_buf` aliasing), §4 (memmon child GCs), §5 (serve-loop latency), §6 (per-child class metadata).

## 3. Edit-mode key consumption is invisible (driver false-FAILs) — OPEN

The Settings NumberPicker edit mode (`graphics/lvgl/edit_mode.rs`,
consulted in `events.rs:494-501`) can consume a key entirely — no Java
queue push, so none of the `key: code=...` dispatch lines added in
`beb0e3d`. Every soak cycle logs two false `FAIL ... no-dispatch-log`
entries for the X/Y presses inside Settings. One `pd_info!` in the
edit-mode consumption branch (e.g. `key: edit-mode consumed pin N`) makes
the last silent drop point observable and lets `scripts/soak/soak-lib.sh`
verify those presses too. Trivial; touch `events.rs` only, then the usual
sim smoke + pre-commit.

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

*2026-09-16:* both still open — `drivers/bme688` has not changed since the
`crates/` move, and it still programs no heater profile (`res_heat_0` /
`gas_wait_0`).

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
