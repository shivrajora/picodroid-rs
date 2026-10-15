# Sim nightly 2026-10-15: follow-ups from the GC-stress and strict-soak lanes

The 2026-10-15 03:00 sim nightly (`2026-10-15_03h00m01s_6d8860d3`) was the first one with the
two lanes `6d8860d3` added: the GC-stress pass and the strict memory soak
(`docs/quality-roadmap.md`). This page records what is still open after that run. The bugs the
lanes found on their first run are fixed and are listed at the end for reference.

Result: PASS 220, FAIL 0, SKIP 44, ERROR 14. All 32 `[no-shrink,gc16]` rows passed, and so did
`mem-diag-strict`, `langsuite` in both modes and `size-ratchet`. The 4 AM HIL fleet on the
same code (`f7652f0d`) passed every row on all three slots, qa_life and threadparity included.
All 14 errors are the net rows.

## Open

### 1. The net rows ERROR: port 7000 already in use

Every `net` row in both shrink modes (netdemo, http_get, https_get, askclaude and three weather
rows) recorded `ERROR <app>[<mode>] (listeners)`. `cron.log` gives the reason:
`net listeners: port 7000 is already in use`. `net-lib.sh::start_net_listeners` refuses to start
when the echo (7000), HTTP (8000) or TLS (8443) port is already taken, and the row is never run.
Before this night, the only refusal in the logs was a HIL WARNING on 2026-10-11 at 04:00:15.

There are two separate collisions:

- **Shrink pass (04:28): caused by the new lanes.** The two lanes added about 35 minutes, so the
  nightly now runs 03:00–04:33 instead of ending around 03:57. The 4 AM `hil-fleet.sh` starts its
  own listeners on the same three ports at 04:00, and its `net-echo.log` shows them up until
  07:43. The sim's shrink-pass net rows now arrive after 04:00, so they fail every night.
- **No-shrink pass (03:15): cause unknown.** Nothing in this repo's logs shows who held 7000 at
  03:15. The HIL fleet had not started, and no `net-echo.log` from another run was written in that
  window, in the main checkout or under `.claude/worktrees/`. A session running net rows or
  `sim-run.sh --coverage` by hand at that hour fits, but there is no evidence for it. By 09:19
  the port was free.

Fix options, cheapest first:

1. Start the sim cron at 02:00 (`crontab -e`, plus the "3 AM" wording in `CLAUDE.md` and
   `docs/`). The run now takes about 93 minutes, so that ends it around 03:35, before the
   fleet starts.
2. Move the lanes that run once a night (`mem-diag`, `mem-diag-strict`, `gc-stress`,
   `sched-diag`, `size-ratchet`, about 45 minutes together) from the no-shrink pass to the end of
   the run, after the shrink pass. The shrink pass's net rows then finish by about 03:45.
3. Have `start_net_listeners` wait for the ports, under a host-wide lock shared by `sim-run.sh`
   and `hil-run.sh`, instead of refusing. That also covers the unexplained 03:15 case. It needs a
   timeout, because the HIL fleet holds the ports for hours.

The ports are compiled into the apps (`NetTestConfig`, and the 7000 in `NetDemo`), so giving the
sim its own port numbers is not a quick change.

### 2. `test-memdiag.sh` kills every simulator on the machine

After each soak, `soak_test` and both self-tests run `pkill -x picodroid` to reap a sim binary
orphaned by `timeout`. That kills every process named `picodroid`: a peer session's
`sim.sh`, its `sim-run.sh` rows, or a GC-stress row running at the same time. It fires up to
three times per lane, in `mem-diag` and in `mem-diag-strict`. The nightly is safe
because it runs the lanes one at a time, but a by-hand run beside other work is not.

Fix: record the sim's pid (run the binary directly, as `sim-run.sh::run_test` does, instead of
going through `sim.sh`) and kill only that process, or run it under `setsid` and kill the process
group.

### 3. TimeDemo depends on the state of the simulator's filesystem image

`examples/timedemo` (part of langsuite) checks that the default zone, after
`TimeZone.setDefault(null)`, is UTC. The simulator reads the default zone from `/system/time` on
its filesystem image. Without `PICODROID_SIM_FS`, that image is the developer's shared one,
`crates/picodroid-core/target/sim-fs.img`. A zone picked in Settings, or left there by the
settings lane before `6d8860d3`, fails the check. `sim-run.sh` now gives its rows and the
settings lane images of their own, so the nightly is unaffected. A local
`./scripts/sim.sh --app langsuite` still fails on an image that holds a zone. Today's shared
image still holds the GMT-12 zone the settings lane left there.

Fix: delete the shared image (it is recreated empty), and either have `sim.sh` give test apps a
fresh image or make TimeDemo restore the stored zone before it checks. The check itself is
correct: on a device with no zone set, the default is UTC.

## Fixed (for reference)

| Commit | Bug | How it showed |
|---|---|---|
| `8a8aaf92` | A pushed Activity was held only in a native local from `instantiate_component` until `push_activity`, while the covered Activity's `onPause` ran Java. A collection there swept it. The Push's intent and every other freshly built component had the same gap. | qa_life `[gc16]`: `Activity lifecycle error: InvalidReference` ×3, then a hang. Every run. |
| `00458e2d` | A finished thread's native tail called `terminate_by_obj(this)` after `exit0` had already dropped its entry. If a collection had swept that Thread and a sibling's new Thread had taken its id, the call deleted the sibling's entry, which was its only root. | qa_life `[gc16]`: `Thread.start: … left the interpreter: NoSuchMethod` (`OptionsMenu.run`). 2 runs in 48 under load before the fix, 0 in 96 after. |
| `6d8860d3` | The settings lane left a GMT-12 zone in the shared sim filesystem image, and later langsuite runs inherited it. | `langsuite[shrink]` FAIL in the 2026-10-13 and 2026-10-14 nightlies (TimeDemo "default zone starts at UTC"). |
