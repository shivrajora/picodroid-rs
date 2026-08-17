# picoenvmon Long-Soak Plan — catch crashes & regressions (2026-08)

A runbook for an agent session whose job is to soak picoenvmon on the
**`pico_enviro_mon_w`** device (Enviro+ Pack on Pico 2 W) for hours, reproduce
the open P0 panic, and validate heap stability. Read together with
`docs/perf-memory-handover-2026-08.md` (§1 panic, §5 gates) and
`docs/memory-diagnostics.md`.

Primary objectives, in order:

1. **Reproduce / trap the P0 panic** (`range end index 388 out of range for
   slice of length 336`) with offensive mem-diag armed so it fires at damage
   time, not at the late symptom.
2. **Heap gate part B**: the 15-min-plus device soak the panic cut short —
   go-criteria: no OOM lines, min-ever native free ≥ 40 KB, `lblk` ≥ 16 KB at
   matched idle after the soak, no unexplained reboot.
3. **Collect the PEM-3 retune inputs**: `memmon: storage` steady state
   (obj/arr/str chunks, fields, arena) with the network thread running, to
   re-derive `[jvm] prereserve_*` for this board.

---

## 1. Build & flash

```bash
# Offensive mem-diag release build (primary soak build).
# mem-diag on device is a Cargo feature via PICODROID_EXTRA_FEATURES — flash.sh
# has no -m flag. OFFENSIVE poisons freed spans and panics at the moment of
# damage (live field holding poison / post-GC integrity failure).
env $(grep -v '^#' .wifi-creds.env | xargs) \
  PICODROID_EXTRA_FEATURES=mem-diag PICODROID_MEMDIAG_OFFENSIVE=1 \
  ./scripts/flash.sh --board pico_enviro_mon_w --app picoenvmon --release \
  > /tmp/soak-rtt.log 2>&1 &
```

Operational rules (violating these wastes hours — all learned the hard way):

- **flash.sh never exits** — it stays attached streaming RTT. Always
  background it with output redirected to a file; that file IS your device
  log for the whole soak.
- **Never overlap flash attempts.** A leftover probe-rs process holds the USB
  claim ("Failed to open probe"). To detach: `pkill -f 'probe-r[s]'` — the
  bracket trick is mandatory; a literal `pkill -f probe-rs` matches your own
  shell's command line and self-kills (exit 144).
- **Never run sim.sh and flash.sh concurrently for the same app** — the
  shared `build/apks/picoenvmon.papk` races and a shrink mismatch bricks boot
  with `FrameworkVersionMismatch`.
- **Verify the RTT ELF matches the profile**: flashing `--release` and
  attaching with a debug ELF silently decodes zero RTT. Using flash.sh's own
  attach (as above) avoids this.
- Boot-loss recovery: `./scripts/power-cycle.sh` (auto-detects the hub via
  the CMSIS-DAP probe). Expect the LittleFS `bootcount` to advance +1 per
  reflash, +2 when power-cycling between reflashes.

## 2. The nightly cron WILL interfere — plan around it

From project memory (`reference_nightly_cron`): **3 AM** sim-run, **4 AM**
hil-run *on the attached board*, 9 AM submodule check. Two hazards for an
overnight soak:

1. `hil-run.sh` internally runs `pkill -f probe-rs` — at 4 AM it will kill
   your RTT attach (and its own flashing may target the attached probe;
   `hil-run.sh` hardcodes `BOARD="testbench_rp2350"` but the probe is shared).
2. Any wrapper shell you leave running with "probe-rs" in its command line
   dies with it (exit 144).

Choose one: (a) run the long soak during the day and end before 3 AM; (b)
disable the cron entries for the night; or (c) accept losing RTT at 4 AM and
rely on the external liveness probes (§4) — the firmware keeps running when
the probe detaches; reattach afterward with a fresh
`probe-rs attach`-equivalent (re-run flash.sh only if you intend to reflash).
Record which option was taken; a 4 AM RTT gap is otherwise indistinguishable
from a hang in the logs.

## 3. Load profile

The P0 panic needed **combined** load; a bare curl loop soaked clean for
minutes repeatedly. Reproduce all three stressors together, then vary:

```bash
# A. Browser-realistic dashboard load (background for the whole soak)
while true; do curl -s -m 8 http://<device-ip>:8080/ | wc -c >> /tmp/soak-http.log; date +%s >> /tmp/soak-http.log; sleep 2; done &

# B. Nav churn via pdb (device USB CDC — independent of the probe).
#    Keycodes: 19=PREV(A) 20=NEXT(B) 23=ENTER(X) 4=ESC(Y).
#    The documented worst fragmenter is screen churn: Live→back, History→back,
#    Settings→back, Network→back, ~300 ms–1 s gaps. Example cycle:
for k in 23 4 20 23 4 20 23 4 20 23 4 19 19 19; do
  ./scripts/pdb.sh input keyevent $k; sleep 1
done

# C. Periodic perturbations
#  - every ~10 min: open Network, press X (Refresh) → forced NTP+weather fetch
#  - every ~30 min: a burst of 3 concurrent curls (exercises backlog-1/RST)
#  - occasionally: dwell on Live for 5+ min (1 Hz smoothed UI churn), then History
```

Phase plan:

| Phase | Duration | Load | Purpose |
|---|---|---|---|
| Smoke | 15 min | A only | baseline floors; confirm boot, join, NTP, serving |
| Combined | 60 min | A+B+C | the P0 repro window (original crash hit ~4 min in) |
| Quiet hold | 60 min | A only, no input | leak floor / steady-state storage read |
| Long | 4 h+ (overnight if §2 handled) | A + hourly B/C cycles | endurance; 6 h NTP re-sync boundary |

If the panic reproduces in phase "Combined", STOP loading and go to §6.

## 4. What to watch — signals and how to read them

**External liveness (works even without RTT):**

- **Uptime in the dashboard footer is the reboot detector.** It must be
  monotonically increasing across fetches; a reset to `0h 0m ..` = the device
  rebooted (or crashed and watchdog/power recovered it) — treat as a crash
  even if RTT missed it.
- HTTP success rate from `soak-http.log`: page ~700–900 bytes = full page;
  0 bytes = timeout. Isolated timeouts during NTP/weather windows are
  **expected** (§5); sustained zeros = hang or crash.
- `./scripts/pdb.sh ping` as a liveness cross-check when HTTP looks dead.
- Clock sanity: the footer UTC time should track wall time between fetches.

**RTT log — alarming (capture immediately, correlate timestamps):**

- `panicked at` / `HardFault` / `Firmware exited` — the probe-rs attach also
  EXITS on these, so the flash.sh background task ending IS itself an alarm.
- `mem-diag: live object ... holds poison` / `post-GC integrity violation` —
  the offensive-mode trap firing; this is the P0 payoff, the message names
  the damaged object at the moment of damage.
- `OOM: tried N B — free ..., largest block ...` — record the whole line; the
  canonical regression signature is a doubling-realloc (~2× a table size)
  against a fragmented largest-block.
- `Thread.start: child-task ... failed: <JvmError>` — the network thread died;
  the dashboard will go dark ~permanently (no respawn). InvalidReference /
  InvalidBytecode here historically meant heap corruption or an untested
  bytecode shape — see `project_jvm_concurrency_gc_fixes` memory.
- `native miss` — a NoSuchMethod-class dispatch failure.
- `LEAK? native floor rose ...` — memmon's growth sentinel; one firing during
  warm-up is tolerable, repeated firings on a flat workload are not.
- `http: connection error` / `http: unexpected` spikes; `bind failed` loops
  after boot (once-at-boot with recovery is fine).
- ALERT storms: alerts are edge-latched; more than one line per actual
  threshold crossing means the latch broke.

**RTT log — memmon lines to RECORD (phase floors for §7 deliverables):**

- `memmon w=.. live=.. nused=.. nmin=.. lblk=.. frag=..` — track `nmin`
  (min-ever free), `lblk`, `frag` at the end of each phase, at matched idle.
- `memmon storage w=.. obj_chunks=.. arr_chunks=.. str_chunks=..
  fields_cap=.. arena_cap=..` — the steady-state values during the quiet hold
  are the PEM-3 prereserve retune inputs.
- **Known gap:** GCs triggered in the network thread's executor do NOT appear
  in memmon's `gc=`/`freed=` columns (child handlers swallow `report_gc` —
  handover doc §3). `live` oscillating with `gc=+0` is that gap, not a bug.

**Expected noise — do NOT chase these:**

- Boot-time `cyw43: ioctl cmd 263 error status -5` ×2 (`apsta`,
  `ampdu_rx_fac`) — NET-1, cosmetic, documented.
- `bme: ... gas=12887828` constant — BME688 heater profile never programmed
  (known; IAQ tile cosmetic).
- Pressure reading ~3600 hPa (raw `press=356850`) — physically implausible,
  **pre-existing driver/compensation artifact**, not a soak regression. Worth
  a separate ticket, not this session's problem.
- Weather content nonsense ("Blizzard" in summer) — upstream wttr.in cache
  garbage; the pipeline is verbatim. Never assert on weather content.
- Isolated page timeouts at boot / 6 h / 15 min marks — §5.

## 5. Known serve-loop gaps (expected, timestamp-correlatable)

NTP and weather run on the serve thread's housekeeping tick. The dashboard
does not accept connections during: boot-time NTP retries (up to 3 s × 3),
weather fetch (DNS + HTTP, up to ~5 s), the 6 h NTP re-sync, the 15 min
weather refresh, and 5 min failure backoffs. Log the device's `ntp:` /
`weather:` lines and correlate: an HTTP timeout **at** one of these
timestamps is expected behavior; one **away** from them is a finding.

## 6. On crash — evidence before recovery

1. Preserve `/tmp/soak-rtt.log` (copy it out immediately — probe-rs printed
   the panic + both cores' backtraces into it before exiting).
2. Note the last 30 s of context: which load was active, last memmon line,
   last `ntp:`/`weather:`/`GET` line, HTTP log state, uptime at last fetch.
3. Do NOT immediately reflash. If deeper state is needed, use the
   gdb-multiarch + probe-rs gdb attach recipe
   (`project_handle_dangle_sim_blind` / `reference_gdb_sim_debugging`
   memories) against the halted target.
4. Only then reflash (or power-cycle) and decide: resume the soak to test
   reproducibility, or pivot to fixing with the captured evidence.
5. File findings: append to `docs/picoenvmon-qa.md` (the P0 section) and
   update the `project_picoenvmon_wifi_showcase` memory.

## 7. Deliverables of the soak session

- Verdict per phase against the go-criteria (objective 2), with the memmon
  floor numbers as evidence.
- Either a trapped P0 (offensive-mode message + backtrace + repro recipe) or
  a documented N-hour clean run under the combined load — both are results.
- The quiet-hold `memmon: storage` steady state → proposed new
  `[jvm] prereserve_*` values for `platforms/rp/boards/pico_enviro_mon_w/board.toml`
  (PEM-3 method: steady state + one growth step of margin each).
- Log artifacts kept (RTT + HTTP logs with timestamps).
- QA doc + memory updates; if code changed, the CLAUDE.md check discipline
  applies (sim smoke trio + full `./scripts/pre-commit`) before commit.

## 8. Quick reference

```bash
# device IP: RTT prints "net: up, ip a.b.c.d" (192.168.1.218 on this LAN so far)
# sensor rows: live from boot since cd6f989 (Logger auto-starts; switch = off-toggle)
# detach probe safely:            pkill -f 'probe-r[s]'
# orphan sim (if sim used):       pkill -x picodroid    # 8080 orphans block bind
# hard recovery:                  ./scripts/power-cycle.sh
# liveness:                       ./scripts/pdb.sh ping
# input injection:                ./scripts/pdb.sh input keyevent <19|20|23|4>
# sim counterpart of the soak (optional cross-check; NOT while flashing):
PICODROID_MEMDIAG_OFFENSIVE=1 ./scripts/sim.sh -b pico_enviro_mon_w -a picoenvmon -l 360 -m
```
