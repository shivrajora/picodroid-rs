# Completed: PicoClock roadmap

Items closed out of [picoclock-roadmap.md](../designs/picoclock-roadmap.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

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
