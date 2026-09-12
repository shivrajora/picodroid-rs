# PicoClock roadmap

Companion to [picoclock-2026-09.md](picoclock-2026-09.md), which describes what
the app is and why it is shaped that way. This file is the list of what it is
not yet. Everything here is unstarted except R1, which is done and kept below
for what it left behind.

Items are grouped by what they change rather than by size, and ordered within
each group by how much they are worth.

One complaint about this app turned out not to be about this app at all: the
Set-time screen scrolls at 3-8 fps and tears, for reasons that live in the
framework's render and input path rather than in `picoclock`. That is profiled
and scoped separately in
[scroll-performance-2026-09.md](scroll-performance-2026-09.md); nothing about it
is an app change, so nothing about it is listed below.

## 1. The thing that stopped it being a real alarm clock

### R1. Alarms that survive leaving the app — DONE 2026-09-11

The long path was taken: a framework-level `picodroid.app.AlarmManager`, written
up in [alarm-manager-2026-09.md](alarm-manager-2026-09.md). The alarms live in a
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

## 2. Making the clock right

### R2. Network time

The board has WiFi and `examples/picoenvmon` already carries an SNTP client. A
sync button on the set-time screen, and an automatic sync on the first
successful join, would turn the manual set from the only path into a fallback.
This matters more here than on hardware with a battery-backed clock, because
every power cut currently ends with a user turning a stepper.

Worth doing alongside: a "last synced" line, and a periodic re-sync, since a
free-running oscillator drifts.

### R3. Say when an alarm was missed

After a power cut the clock comes back unset, and an unset clock cannot fire
anything. The user has no way to learn that an alarm was skipped. Recording the
last-armed time before the lights went out, and comparing on the next clock set,
would let the face say so.

### R4. Nothing about daylight saving

Deliberately absent, and the note here is to keep it that way unless someone
wants to own it. There is no timezone database on this platform, so support
would mean hand-maintained rules that go stale silently. A fixed offset that the
user moves twice a year is worse than it sounds only if the app pretends
otherwise.

## 3. The ring

### R5. Give up eventually

Also where R1's leftover belongs: HOME during a ring silences it for good,
because the app goes down with it. An alarm re-armed a minute out as the app is
torn down would survive that, and is the same machinery as giving up after
fifteen minutes.

The ring currently sounds until somebody taps it. Android silences after about
fifteen minutes and records a missed alarm. On a board with a buzzer and no
volume control, an alarm nobody is present for is a fire alarm.

### R6. A gentler wake

One fixed two-tone pattern at one volume. A duty-cycle ramp over the first
thirty seconds is a small change to `Buzzer` and a large change to being woken
by it. Choosing a tone, or a pattern per alarm, follows from the same work.

### R7. Snooze that knows how many times

A snooze also does not survive the app being torn down and started again by its
own alarm (R1): the framework keeps the alarm, not the fact that it is a
snooze, so the count would have to be stored with the alarm to mean anything.

The snooze is a fixed nine minutes with no count. Showing "snoozed 3 times", and
optionally shortening or refusing after a few, is what stops the snooze being a
way to sleep through the alarm entirely.

## 4. Screens

### R8. Alarm labels

`Alarm.label` is stored, displayed and respected everywhere in the app. Nothing
sets one, because setting one means the soft keyboard on a screen that is
already full. The editor needs a row that opens the keyboard, and probably needs
to scroll.

### R9. Twelve-hour display

The face is hardcoded to twenty-four hour. `TimePicker` already supports both
through `setIs24HourView`, so the SDK is not the obstacle: it needs a setting, a
place to put it, and an AM or PM marker on the face, which the seven-segment
layout currently has no room for.

### R10. A timer and a stopwatch

The Android clock app has both and neither needs the alarm machinery. A
stopwatch is the cheaper of the two and reuses the segment face directly. A
countdown timer wants the same ring path as an alarm, so it is worth doing after
R5 and R6 rather than before.

### R11. Somewhere to put settings

R5, R6, R7 and R9 each want a switch. There is no settings screen, and the
clock face has room for one more button.

## 5. Inside

### R12. Test the widget slot tables

Done for the alarm table, not for the widgets: `crates/picodroid-core/src/alarms.rs`
was deliberately written with no LVGL dependency so `cargo test` reaches it, and
carries fifteen tests. The widgets below still have none.


The TimePicker slot leak fixed alongside this list would have been caught by a
unit test over `register_picker` and the release path, and was not, because the
LVGL widget modules are `#[cfg(not(test))]` and anything written inside them is
silently never run. Making that testable means lifting the slot bookkeeping into
a module with no LVGL dependency and reaching it through a `#[cfg(test)]
#[path]` shim, then confirming with `cargo test -- --list` that the tests
actually exist. This is worth doing once, for every widget that keeps a table.

### R13. Audit the other widgets for the same leak

`TimePicker` kept two hand-rolled static tables and freed neither on widget
delete. The widgets that use `PtrMap` are fine, because it carries its own
delete trampoline. `date_picker`, `spinner`, `alert_dialog` and `keyboard` each
hold static state and register one delete hook; whether that hook covers
everything they allocate has not been checked. One pass over them would settle
it.

### R14. A second self-test tier for the store

`SelfTest` covers `Clock` and `AlarmSchedule`, which is where the silent failures
live. `AlarmStore` has none: the slot bitmask, the key naming and the delete path
are only exercised by hand. They are pure enough to check the same way, given a
preferences file the test can scribble on.

### R15. Fewer full rebuilds of the alarm list

`AlarmListActivity` rebuilds every card on every `onResume`, including a return
from a cancelled edit that changed nothing. It is correct and it matches the way
the launcher and settings build their rows, and at eight alarms it is not slow.
It is still more work than the screen needs, and the generation counter that
makes the rebuild safe would also make a narrower update safe.

## Not planned

- **A world clock.** It needs a timezone database, which is R4's problem again.
- **Alarm sounds from files.** The buzzer is a passive sounder on one pin, not a
  speaker.
- **Syncing alarms anywhere.** There is nothing to sync with.
