# PicoClock roadmap

Companion to [picoclock-2026-09.md](picoclock-2026-09.md), which describes what
the app is and why it is shaped that way. This file is the list of what it is
not yet. Nothing here is started.

Items are grouped by what they change rather than by size, and ordered within
each group by how much they are worth.

One complaint about this app turned out not to be about this app at all: the
Set-time screen scrolls at 3-8 fps and tears, for reasons that live in the
framework's render and input path rather than in `picoclock`. That is profiled
and scoped separately in
[scroll-performance-2026-09.md](scroll-performance-2026-09.md); nothing about it
is an app change, so nothing about it is listed below.

## 1. The thing that stops it being a real alarm clock

### R1. Alarms that survive leaving the app

Today the alarms are only watched while picoclock is the foreground app.
Pressing HOME tears every Activity down and rebuilds the JVM from the next
package image, and `AlarmService` dies with it. An alarm clock that forgets its
alarms when you glance at something else is not one.

Two ways out, and they are not alternatives so much as a short path and a long
one.

**Short: make the board boot into the clock.** Multi-app already supports a boot
default, which is how the sim log reads `[packages] boot: picoclock (boot
default)`. On a board dedicated to being a clock this is the honest answer, and
it costs a line of configuration. It does not help a board that also runs other
things.

**Long: a framework-level alarm.** Android has `AlarmManager` for exactly this
reason: an app should be able to ask to be woken at a time without staying
resident. On this platform that means a scheduled wake-up owned outside any
package image, surviving an app switch, and re-entering the owning app when it
fires. That is a design conversation about the multi-app supervisor, not a
picoclock feature, and it would serve every app that wants to do something
later. Worth writing up separately.

Until one of those lands, the app should at least be honest about it. A line on
the clock face saying alarms only run while the app is open would cost nothing
and mislead nobody.

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

The ring currently sounds until somebody taps it. Android silences after about
fifteen minutes and records a missed alarm. On a board with a buzzer and no
volume control, an alarm nobody is present for is a fire alarm.

### R6. A gentler wake

One fixed two-tone pattern at one volume. A duty-cycle ramp over the first
thirty seconds is a small change to `Buzzer` and a large change to being woken
by it. Choosing a tone, or a pattern per alarm, follows from the same work.

### R7. Snooze that knows how many times

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
