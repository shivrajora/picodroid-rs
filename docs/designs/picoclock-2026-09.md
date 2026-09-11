# PicoClock — a clock and alarm app for pico_touch_kit

Status: landed 2026-09-11. Lives in `examples/picoclock/`.

## What it is

A clock and alarm app for the 52Pi EP-0172 carrier and its 320x480 capacitive
panel: a clock face, a list of alarms, an editor for one, a screen for setting
the wall clock, and a full-screen ring with snooze and stop. The carrier's
buzzer on GP13 and its LED on GP16 sound and flash while an alarm is up.

It is the first app written against `pico_touch_kit`, and most of what follows
is a consequence of that board rather than of clocks.

## What the board decides

**No RTC.** The RP2350B has no battery-backed clock and the carrier adds none,
so `System.currentTimeMillis()` counts from zero at every cold boot. A clock app
therefore has to be able to say what time it is — hence the Set-time screen,
and `Clock.isSet`, which is why the face says "clock not set" rather than
showing 1970 as though it meant something. Alarms are stored in flash and
survive the power cut; the time is not, and cannot be.

**The panel stays lit.** `board.toml` sets `idle_timeout_ms = 0`, because on
this board neither half of the idle-sleep path understands touch (4f445b6). A
clock whose face blanks after a minute would be pointless, so the app depends on
that setting; restoring the default here would need the touch wake-up work that
the board file already describes.

**Two buttons, both system keys.** BTN1 is BACK and BTN2 is HOME, and neither
drives focus navigation. Nothing in the app is reachable by a focus ring, so
every target is sized for a fingertip (`Ui.TAP_HEIGHT`, 56 px, about 9 mm at
this panel's 165 dpi) and the header carries its own back chevron.

## The parts

| Class | What it owns |
| --- | --- |
| `Clock` | Epoch milliseconds to and from a local civil date, and the display strings. Integer only; there is no `Date`, `Calendar` or `TimeZone`, and no timezone database — "local" is UTC plus one offset the user picks. |
| `Alarm` | A mutable struct: hour, minute, armed, a Sunday-first day mask, a label. |
| `AlarmSchedule` | When an alarm next rings. Pure arithmetic over `Clock`, no state and no clock read of its own. |
| `AlarmStore` | The alarms and the offset, in `SharedPreferences`. Read once, written through on every edit. |
| `AlarmService` | The heartbeat. Watches the wall clock, rings, drives the buzzer, and hands the screens their tick. |
| `Buzzer` | GP13 through `Pwm` and GP16 through `Gpio`, best effort. |
| `SelfTest` | 61 checks over `Clock` and `AlarmSchedule`, run at startup. |
| `ui/*` | Five screens, a shared palette and widget factory, and the seven-segment face. |

## Decisions worth the words

### One heartbeat, shared

`AlarmService` runs a thread that sleeps 250 ms and posts. Nothing else in the
app owns a clock: the screens redraw off `Listener.onTick`. There is no
`Handler`, `postDelayed`, `Timer` or `ScheduledExecutorService` here, and the
two documented alternatives — a thread that sleeps and posts, or an animation's
end action — would each cost a thread or an animation slot *per screen*. The
alarms already need a heartbeat, so the face uses that one.

What it must not be is a Runnable that re-posts itself to `Executors.mainExecutor()`.
The main loop is a single FIFO interleaving LVGL ticks with posted Runnables, so
such a Runnable is re-popped immediately rather than once per frame, and spins
the UI thread flat out between ticks.

The tick notification is one pre-allocated `Runnable`, reposted every tick. A
fresh lambda would be four short-lived objects a second for as long as the board
is up.

### The face is drawn, not typed

There is no text-size API — a `TextView` is the bundled font and nothing else —
so "HH:MM" big enough to read across a room is 28 rectangles and two dots, as a
seven-segment display. It is also the cheaper way to run one: a redraw is a
colour change on the segments that actually differed, where re-setting a label
re-lays-out and re-rasterises the whole string every second.

Each view costs the RP2350 roughly 7 ms of LVGL work, so the face is built a few
segments per UI tick rather than all at once, the same way the launcher and the
settings app build their rows. The alarm list does the same with its cards.

### A missed alarm does not ring

`AlarmSchedule.shouldRing` requires the fire to be due *and* no more than a
minute late. A tick that lands a second past the instant is normal; one that
lands an hour past it means the wall clock jumped — a sync, or the user setting
the time — and ringing then would be an alarm for a moment that never happened.
The service re-arms such an alarm instead.

### Three framework shapes the app had to work with

**A native upcall resolves methods on the exact class only.** `find_method_by_name`
does not walk the superclass chain; `invoke_lifecycle` compensates with a
two-step, trying the concrete Activity and then the SDK's `Activity`, and
nothing in between. So a callback declared only on `BaseActivity` is never
found: the screens each re-declare `onResume`, `onPause` and `onDestroy` as
one-line overrides, and those are load-bearing rather than ceremony. The same
rule is why `ServiceLink` is a class of its own rather than something
`BaseActivity` implements — a `ServiceConnection` inherited from a base class
binds and then silently never connects.

**`DatePicker` selects a day and nothing else.** It is an `lv_calendar` with no
header, so it cannot move off the month it is showing, and `getDay()` returns
zero until a cell is tapped — accepting the day already highlighted would
otherwise set the clock to the day before the month began. The Set-time screen
brings its own year and month steppers and tracks the chosen day itself.

**A Service cannot start an Activity.** `startActivity` is on `Activity`, not
`Context`, deliberately. A ring is therefore routed by whichever screen is
resumed: the listener is claimed in `onResume` and released in `onPause`, so
exactly one screen reacts however deep the stack is.

## Testing

The screens need a finger; the arithmetic under them does not, and that is the
half that fails silently. `SelfTest` checks the calendar conversions, the
weekday derivation, the formatting and the scheduler against hand-computed
instants, and logs one line. `scripts/hil-tests.conf` carries a `sim` row pinned
to `pico_touch_kit` that asserts on it.

The screens themselves were driven through the simulator's control FIFO
(`PICODROID_SIM_CTRL_FIFO`, `input tap` / `input swipe`), which covered: setting
the clock, creating an alarm, the repeat toggles, the arm switch, cancel,
delete, the ring firing on time, snooze, the snooze re-firing, stop, and the
alarms surviving a restart.

## Not done

- **No network time.** The board has WiFi and `SntpClient` exists in
  `examples/picoenvmon`; a "sync now" button on the Set-time screen is the
  obvious next thing and would make the manual set a fallback rather than the
  only path.
- **No alarm labels in the UI.** `Alarm.label` is stored, displayed and
  respected everywhere; nothing sets it, because doing so means the soft
  keyboard on a screen that is already full.
- **No timer or stopwatch.** The Android clock app has both.
- **The buzzer pattern is fixed.** No volume, no tone choice, no gradual wake.
