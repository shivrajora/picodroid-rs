# The platform time service: who sets the clock, and in what zone

**Status: built 2026-10-09** (decision F5 of
[claudeusage-decisions-2026-10.md](claudeusage-decisions-2026-10.md)). Amendments at the bottom
record where execution diverged from the plan.

## 0. Why

Boards have no battery-backed clock, so `System.currentTimeMillis()` counted from boot until
something called `SystemClock.setCurrentTimeMillis`. That something was each app: `https_get`,
`weather` and `askclaude` each ran an `SntpClient` exchange before their first request (the TLS
verifier fails closed without a clock), `picoenvmon` carried a private SNTP client and a six-hour
re-sync job in both its Java and Kotlin twins, `claudeusage` took the bridge's time, and
`picoclock` asked the user to turn a stepper after every power cut (picoclock-roadmap R2). Each
of them also decided its own zone: a `TimeFormat` static here, a `SharedPreferences` key there.

Android does not let an app set the clock. The platform anchors it from the network once
connectivity is up, re-anchors on a schedule, and the zone is a user setting that
`TimeZone.getDefault()` reports to every app. This doc adds that shape.

## 1. What a user sees

- A board with a network has the right time within seconds of its join, on every boot, with no
  app involved. An `https` request made before the first exchange has landed waits for it (up to
  8 s) instead of being refused.
- **Settings → Date & time**: "Automatic date & time" (on by default; off, and only a hand-set
  clock applies), "Time zone" (a list from UTC-12:00 to UTC+14:00, whole hours and the fractional
  zones in use, the current one marked), and the time now in that zone, or "not set".
- `picoclock` shows the right time after a power cut without a visit to Set-time; its Set-time
  screen stays as the fallback for a board with no network, and the zone it shows is the platform's
  unless the user picked one there.
- The log says what happened: `time: synced from pool.ntp.org (162.159.200.1), rtt 38 ms, step
  0 ms` on each anchoring (the step is how far the clock moved from where it was), `time: zone
  UTC+05:30` and `time: automatic off` on the settings.

## 2. The Android-facing API

Nothing new for apps to call; three things to stop calling.

| Android | Here | Notes |
|---|---|---|
| `TimeZone.getDefault()`, `ZoneId.systemDefault()`, every `now()` | The platform zone | Was a per-process UTC static. `setDefault` still overrides for the process, as on Android. The id is `UTC` or `GMT+hh:mm`. |
| `Settings.Global.AUTO_TIME` | `picodroid.provider.Settings.Global.getInt/putInt` | New nested class; one setting. Default 1. |
| `AlarmManager.setTimeZone(String)` | Stores the platform zone | A fixed-offset id (`UTC`, `GMT+05:30`, `+01:00`, `-0330`, `Z`); a region id throws `IllegalArgumentException`, since there is no tz database to resolve it. Works on every board, not only multi-app ones. |
| `AlarmManager.setTime(long)` | `SystemClock.setCurrentTimeMillis` | For completeness; Settings has no manual set (§6). |
| `SystemClock.setCurrentTimeMillis` | Unchanged, public | Android keeps it public behind `SET_TIME`; permissions are not enforced here (manifest decision F4 says so), and the next automatic sync moves the clock back to the network's. Apps no longer need it. |
| `SntpClient` | Unchanged | Kept for an app that wants its own server or the round-trip figure. |

## 3. How it works

### 3.1 The task

`crates/picodroid-core/src/time_service/task.rs`, one task per boot on every board with
`has_network`, spawned beside the other boot tasks (`platforms/rp/src/boot_tasks.rs` after the
link task, `sim_boot.rs` after the debug bridge) through the RTOS seam, so it is core-0 pinned
like every core task and charged to the boot budget from one constant
(`time_service::TASK_STACK_BYTES`, 4 KB; the boot-budget model has a `timesync` row under
`has_network`). Priority: the background tier below the interpreter on the device; the JVM tier in
the simulator, for the same starvation reason the TLS handshake task has (`net/tls.rs`).

The loop: sleep up to a second; on each wake read `LINK_CHANGES.generation()` and
`is_network_up()`. A fresh link means "sync now" with a clean retry ladder. When the link is up,
automatic time is on and the due time has passed: resolve `pool.ntp.org` (afresh each time, so
the pool rotates), one UDP exchange with a 3 s receive timeout, anchor. Success schedules the next
exchange six hours out; failure walks 5 s, 15 s, 60 s, then five minutes. A kick
(`request_sync_now`) from a JVM task brings the due time forward: the TLS layer uses it when a
handshake finds the clock unset, Settings when automatic time is turned back on.

Why polling and not a wake-up from the link driver: the simulator's link events come from host
threads outside the kernel (the Wi-Fi fake's join verdict is a `std::thread`), where no kernel
primitive may be touched; a one-second poll on a task that is otherwise asleep for hours costs
nothing anyone can measure, and the sched-diag monitor does not count a 1 s sleep as polling.

### 3.2 The exchange

`time_service/sntp.rs` is the codec, host-tested: a 48-byte v4 mode-3 request whose transmit
field carries a nonce; the reply must be mode 4, leap indicator not 3, stratum 1–15, echo the
nonce in its originate field, and carry a non-zero transmit timestamp. The anchor is the transmit
timestamp plus half the round trip, with the full 32-bit fraction (the Java `SntpClient` uses the
top byte); NTP era 1 (after 2036) is handled. Stray datagrams and late replies to an earlier
nonce are skipped within the one receive timeout.

### 3.3 The clock

`os/system_clock.rs`'s seqlock already ran its three stores inside an `AtomicSection`, which
suspends the scheduler; the comment said "single writer" from before the section existed. Two
writers (the task re-anchoring while an app or Settings sets the clock) cannot interleave their
halves and the later one wins, so the comment was corrected rather than a mutex added.

### 3.4 The zone store

`/system/time`, beside `/system/wifi` and `/system/display`: eight bytes, `[i32 LE offset
minutes][u8 auto time][3 reserved]`, written to a temporary and renamed over. Missing or short
means UTC and automatic. Cached in atomics (the task reads `auto_time` from its own task; the
natives run on JVM tasks). Range: UTC-12:00 to UTC+14:00.

`java.util.TimeZone.getDefault()` reads the offset through one native
(`nativeDefaultOffsetMinutes`, served by the platform handler the way `System.currentTimeMillis`
is, with the class on the JVM's canonicalisation list) and rebuilds its cached `TimeZone` when
the offset moves, so Settings sees its own change at once. `ZoneId.systemDefault()` and the
`now()` methods were already built on it. The id parse for `AlarmManager.setTimeZone` lives in
Rust (`time_service::parse_offset_minutes`) so that `AlarmManager` does not pull `TimeZone` and
`java.time` into a board that excludes them (`testbench_rp2040`).

### 3.5 TLS

`net/tls.rs::handshake` calls `time_service::task::wait_for_wall_clock(8000)` when the clock is
unset and the link is up: a kick, then a 100 ms poll until the anchor lands or the budget is
spent. The `NoClock` message now names the time service rather than `SntpClient`.

### 3.6 The simulator

The host's sockets reach the real pool, so a network board's simulator anchors the way a device
does; `PICODROID_SIM_WALL_CLOCK=1` (the HTTPS rows) is unchanged and simply makes the first
exchange a re-anchor. `PICODROID_SIM_NTP_SERVER=<host>` points the task elsewhere;
`PICODROID_SIM_NTP_SERVER=off` keeps it from syncing, for a run whose clock must stay unset or
must be exactly the host's (the `claudeusage` pixel A/B replays a bridge payload stamped at
hh:mm:05 and relies on the bridge's time).

## 4. The apps

- `https_get`, `weather`, `askclaude`: the `syncClock*` methods and their `SntpClient` imports are
  gone. The TLS wait (§3.5) covers a first request that beats the first exchange.
- `picoenvmon` (both twins): the private `SntpClient` is deleted; the housekeeping job's NTP step
  became a look at whether the clock is set (the 2001-01-01 line `picoclock` draws), every 5 s
  while it is not, every six hours once it is. `isTimeSynced()` keeps its name and its readers
  (the dashboard footer, the Network screen). The weather fetch still waits on it.
- `claudeusage`: sets the clock from the bridge only while it is unset (a LAN with no route out),
  so the pixel A/B harness still works with `PICODROID_SIM_NTP_SERVER=off`; the bridge's zone
  stays the display zone, as the decision doc allowed, because the PC knows the user's zone and
  `TimeFormat` deliberately avoids `java.time` classes for heap.
- `picoclock`: `AlarmStore.offsetMinutes()` defaults to the platform zone when the user never
  picked one on Set-time; the Set-time screen is otherwise unchanged and is the fallback R2 asked
  for. The alarm service already re-schedules on a clock jump (`AlarmService` checks for an
  armed instant more than an hour past).

## 5. What is not here, and why

- **No manual date and time in Settings.** A board with a network gets its time from the
  network; the one app that needs a hand-set clock on a board without one keeps its own screen.
  A `DatePicker` + `TimePicker` page driven by four keys is a day of UI for a path the platform
  exists to make unnecessary. `AlarmManager.setTime` is there if a Settings page wants it later.
- **No `AUTO_TIME_ZONE`.** SNTP carries no zone and there is no geolocation.
- **No permission check on `setCurrentTimeMillis` / `setTimeZone`.** The project enforces no
  permissions (F4 records that); making these two the exception would be a false sandbox.
- **Fixed offsets only.** picoclock-roadmap R4 stands: no tz database, so daylight saving is the
  user moving the zone twice a year. The zone list is offsets, not cities.
- **No `TextClock`.** Listed under the decisions doc's leftovers; a `TextView` that formats the
  time once a minute would now have a zone to format in, so it is the natural next small SDK
  addition.

## 6. Verification

- Host: `cargo test -p picodroid-core time_service` (the codec, the store record, the id parse,
  the setter bounds); the native-table guards (`method_tables`, `class_registry`, the member-name
  literal guard, which now allows the zone word `UTC`).
- Simulator: `./scripts/sim.sh --board testbench_rp2350w --app timedemo` logs `time: synced from
  pool.ntp.org …` within a few seconds of boot; the settings lane
  (`sim-run.sh --app settings`) walks Date & time, picks `GMT-12:00` and toggles automatic time
  off and on (`Settings[]:] date-time`, `time zone GMT-12:00`, `date-time auto 0`).
- Device: the `net` rows of `hil-tests.conf` on `testbench_rp2350w`; the first sync logs the
  task's unused stack (`time: task stack 4096 B, N B unused`), which is the number to read before
  trusting `TASK_STACK_BYTES` on hardware.

## 7. Amendments

- 2026-10-09: the zone-id parse moved from Java (`TimeZone.getTimeZone(id).getRawOffset()`) to
  Rust, see §3.4, after noticing `testbench_rp2040` excludes `java/util/TimeZone` and
  `AlarmManager` is not excluded there.
- 2026-10-09: the task is named `timesync`, not `time`: `time` is an SDK member name and the
  literal guard rejects it as a Rust string.
