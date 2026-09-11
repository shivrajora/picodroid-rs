# Design: AlarmManager — an alarm that outlives the app that set it

Status: landed 2026-09-11. Framework half in `crates/picodroid-core/src/alarms.rs`,
Java half in `sdk/java/picodroid/app/{AlarmManager,PendingIntent}.java`.

Written because [picoclock-roadmap.md](picoclock-roadmap.md) R1 asked for it:
picoclock's alarms were watched by a thread of picoclock's own, and pressing
HOME tears the app down. An alarm clock that forgets its alarms when you glance
at something else is not one.

## What the platform already made hard

One app runs at a time. A cross-package launch — HOME, or `startActivity` with
a package target — unwinds every Activity, destroys the Services, returns from
`run_app`, and the supervisor rebuilds the JVM from the next package image
(`multi-app-2026-09.md` D11). Nothing of the outgoing app survives: the heap is
reset and some thirty module states with it.

Two things do survive, and they are the precedent this is built on: the package
directory (`packages.rs`, a module static the reset does not touch) and the
wall-clock anchor (`os/system_clock.rs`). An alarm has to live in the same
place — outside every app — or it cannot outlive one.

## The shape

**A fixed table of twelve rows**, `alarms.rs`, no allocation, `AtomicSection`
around every entry point. Each row copies what it needs as bytes: the owner's
package name, the target Activity's class name, a request code, a trigger, and
up to two int extras. Copies rather than references, because an app's strings
die with it and a PAPK image moves on install.

**A poll on the UI tick**, `lifecycle.rs::dispatch_alarms`, after widget
dispatch so an alarm never lands between a widget's callbacks. It returns at
most one action per tick, so a frame queues at most one lifecycle op.

**Delivery through Java.** The poll hands the row to `AlarmManager.fireAlarm`,
a static the framework upcalls (`dispatch_sites.rs::ALARM_FIRE`). That builds
an `Intent` and calls `startActivity` — the same native every app calls, so
class canonicalisation, the push op and the GC rooting of the Intent are the
existing ones rather than a second set. Nothing native holds a heap reference.

## Decisions worth the words

### Two states, and why a delivery cannot be recalled

A row is `Armed` or `Firing`. Arming, replacing and cancelling work on `Armed`
rows only; the moment an alarm triggers it becomes `Firing` and leaves the
identity space.

That is not fastidiousness, it is the cold relaunch. The alarm comes due while
the launcher is up, so the framework asks for the owner and tears the launcher
down. The owner starts: its Application `onCreate` runs, its service reads its
own store and re-arms every alarm it knows about — *including the one being
delivered*, now scheduled for its next occurrence. If that `set` could replace
the triggered row, the app would cancel its own wake-up on the way in and the
alarm would never ring. With the split it writes a new row and the delivery
lands a tick later. The simulator shows both, in order:

```
[alarm] wake alarmdemo
[alarm] set alarmdemo alarmdemo/WokeActivity#1 at 6009   <- the fresh one
[alarm] fire alarmdemo alarmdemo/WokeActivity#1 late 26 ms  <- the one in flight
```

### One launch per run, and a limit on trying

A `Firing` row records the run generation its wake-up was asked in
(`packages::run_generation`). While that generation stands the poll asks for
nothing more: the launch is in flight and the app is on its way. A new
generation with the owner still not running means the app ran and never took
it, and after two such attempts the row is dropped with a line in the log. A
service-only app, or one that faults before its first tick, therefore costs two
launches rather than an endless loop between itself and the launcher.

### Two clocks, kept apart

`RTC*` triggers are compared against the wall clock, `ELAPSED_REALTIME*`
against time since boot, and the row remembers which. Converting the elapsed
ones to wall time at set would have been a line shorter and wrong: a later
`setCurrentTimeMillis` would then drag them with it, which Android never does.
Moving the wall clock forward brings RTC alarms due at once, as on Android;
moving it back leaves them waiting.

### RAM, not flash

An alarm does not survive a reset. Neither does the wall clock on a board with
no battery-backed RTC, so an alarm that did survive would be an instant nobody
could compare against until the user set the time — and Android apps already
re-register after `BOOT_COMPLETED`. Persisting them would have meant inventing
the first framework-owned file on the volume for no gain until network time
lands (roadmap R2).

### Extras: two ints, and the rejection is in Java

The store's rows are fixed, so an alarm carries at most two `int` extras under
keys of at most fifteen characters. `PendingIntent.getActivity` checks that and
throws where the mistake is, rather than dropping the extra silently at the far
end a tick later. Strings and booleans are refused rather than coerced: a
boolean would arrive as an int and read back as absent.

`PendingIntent` flattens the extras itself, through four public-but-internal
accessors on `Intent`, so the natives take scalars. That keeps `Intent`'s field
slots — `targetClassName` at 0 and `packageName` at 6, which `startActivity`
reads by index — out of this path entirely. For the same reason `Intent` gained
no constant: `field_slot_in` counts every field a class declares, statics
included, so three tag constants would have moved `packageName` off slot 6.
`isIntExtra` is a predicate instead.

## Multi-app boards only

The two classes join `MULTI_APP_CLASSES`. A single-app board has nowhere to
come back from, and shipping the API there would offer a delayed Activity start
that this platform deliberately does not have (there is no `Handler`, no
`postDelayed`). `alarms.rs` is `cfg(has_multi_app)`, which is on for boardless
builds, so the host tests cover it.

## What picoclock does with it

`AlarmService` keeps the heartbeat, the buzzer and the snooze, and stops
deciding when an alarm rings. `plan()` hands the framework one operation per
alarm slot, request code = the alarm's id, on every change to the schedule; a
snoozed alarm is armed outside that loop, because a one-shot has disarmed
itself by then and its snooze would otherwise be the one thing nobody re-armed.
`RingActivity` reads the alarm id and the due minute from the Intent and calls
`AlarmService.ring`, which rejects a fire the wall clock jumped over and starts
the buzzer otherwise.

One operation per slot rather than one for the soonest alarm: two alarms set to
the same minute both have to ring, and a single operation can only carry one of
them. The due time rides along as whole epoch minutes because the extras are
ints — a millisecond count does not fit, and an alarm clock works to the minute.

## Testing

`alarms.rs` carries fifteen unit tests over an injected clock: identity,
replacement, cancellation, the caps, the two clocks, the wake state machine,
the give-up, and the lazy drop when an owner is uninstalled. The module has no
LVGL dependency precisely so `cargo test` reaches it, which the widget modules
do not (roadmap R12).

`examples/alarmdemo` is the end-to-end proof and a `term` row in
`hil-tests.conf`. Run with a launcher it arms an alarm, walks out to the
launcher and is started again to receive it; run on its own — which is how the
harness runs one app — it stays up and the alarm arrives in the same run. Both
paths end at the same line. `sim-run.sh`'s `alarm` lane drives the first of
those, since only it shows what the feature is for.

## Not done

- **No hardware wake.** The poll rides the UI tick, so a board that blanks its
  display on an idle timeout stops polling until something wakes it. Not
  pico_touch_kit, whose `idle_timeout_ms` is 0. Bounding that wait by the next
  due time needs a timeout on the GPIO wait the sleep path blocks in.
- **`setRepeating`, `setWindow`, `setAlarmClock`, `getNextAlarmClock`,
  `OnAlarmListener`.** An app repeats an alarm by setting the next one when it
  receives one, which is what picoclock does.
- **`PendingIntent.getBroadcast` / `getService`.** There are no broadcasts, and
  a Service cannot be started from outside its own app.
