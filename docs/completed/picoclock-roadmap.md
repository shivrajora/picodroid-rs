# Completed: PicoClock roadmap

Items closed out of [picoclock-roadmap.md](../designs/picoclock-roadmap.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## 2. Making the clock right

### R2. Network time — DONE 2026-10-09

Closed by the platform rather than the app:
[time-service-2026-10.md](../designs/time-service-2026-10.md). A framework task on every network
board anchors the wall clock from `pool.ntp.org` seconds after the join and every six hours
after, so a power cut no longer ends with a user turning a stepper; the zone is Settings → Date &
time, which `AlarmStore` reads as its default offset. The Set-time screen stays as the fallback
for a board with no network. Not done from the original note: a "last synced" line on the
face (the sync is logged, not shown), and a sync button (nothing to press when it is automatic).

The original text:

> The board has WiFi and `examples/picoenvmon` already carries an SNTP client. A
> sync button on the set-time screen, and an automatic sync on the first
> successful join, would turn the manual set from the only path into a fallback.
> This matters more here than on hardware with a battery-backed clock, because
> every power cut currently ends with a user turning a stepper.
>
> Worth doing alongside: a "last synced" line, and a periodic re-sync, since a
> free-running oscillator drifts.

## 1. The thing that stopped it being a real alarm clock

### R1. Alarms that survive leaving the app — DONE 2026-09-11

The long path was taken: a framework-level `picodroid.app.AlarmManager`, written
up in [alarm-manager-2026-09.md](../designs/alarm-manager-2026-09.md). The alarms live in a
table outside every app's memory, and when one comes due the framework starts
picoclock again and puts `RingActivity` on top. `AlarmService` keeps the
heartbeat, the buzzer and the snooze, and stops deciding when anything rings.

Two things it leaves behind, both small enough to belong with their neighbours
below rather than here:

- **HOME during a ring still silences it.** The app is torn down and the buzzer
  goes with it. Re-arming a minute out on the way down is R5's territory.
- **A snooze does not survive a relaunch.** The framework keeps the alarm, not
  the fact that it is a snooze, so the face shows the regular next alarm until
  it fires. R7 is where a snooze that counts would fix this.
