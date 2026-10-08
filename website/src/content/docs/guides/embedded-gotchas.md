---
title: "Embedded gotchas: writing robust apps"
description: "The Android idioms that behave differently on Picodroid hardware, and the right pattern for each."
---

Picodroid keeps the `android.*` API surface, but the runtime is a Rust JVM on an MCU with a few hundred KB of RAM, no reflection, and a hardware-button input model. The patterns below are the ones an Android developer reaches for by reflex that misbehave here — each one lists the symptom, the wrong and right code, and why.

## BACK on the root Activity exits to the launcher

Symptom: pressing BACK (the Y button) on your home screen quits the whole app and the launcher appears.

That is the intended behavior, and the same as Android's. The default `Activity.onBackPressed()` first pops the Fragment back stack if the Activity has one, and otherwise calls `finish()`, which pops the Activity off the stack. On the root Activity that is the last entry, so the app ends (`onPause -> onStop -> onDestroy`, then its threads and services stop) and the framework starts the launcher again. Leave the root Activity without an override so users have a way out; pushed Activities keep the default too, so BACK there returns to the parent.

```java
// RIGHT: no override — BACK on the root pops the last entry and returns to the launcher.
public class HomeActivity extends Activity { /* ... */ }
```

```java
// Only for a screen that must confirm before leaving: override, and finish() yourself.
@Override
public void onBackPressed() {
  new AlertDialog.Builder(this)
      .setMessage("Discard changes?")
      .setPositiveButton("Yes", (d, w) -> finish())
      .setNegativeButton("No", null)
      .show();
}
```

Overriding `onBackPressed()` as a no-op traps the user in the app, and so does an `onKeyDown` override that consumes `KEYCODE_BACK` without calling `super`: on a four-button board such as the Pico Enviro Mon there is no other way back to the launcher. Only a board that maps a HOME key (`keycode = 3`; `pico_touch_kit` does) has one: HOME goes to the launcher from any screen and no app can intercept it. On single-app firmware (`max_installed_apps = 1`) an app that exits is not restarted until the next install, so there a trapped root is at least harmless. See [button navigation](/guides/button-navigation/).

## setContentView() is mandatory or the screen is blank

Symptom: the Activity runs, no exception is thrown, but the display shows nothing.

```java
// WRONG: onCreate builds a tree but never installs it.
@Override
protected void onCreate(Bundle savedInstanceState) {
  super.onCreate(savedInstanceState);
  LinearLayout root = new LinearLayout(this);
  root.addView(new TextView(this));
  // ...screen stays blank.
}
```

```java
// RIGHT: install the root view.
@Override
protected void onCreate(Bundle savedInstanceState) {
  super.onCreate(savedInstanceState);
  LinearLayout root = new LinearLayout(this);
  root.addView(new TextView(this));
  setContentView(root);
}
```

Why: `setContentView(root)` is the only call that parents your tree to the LVGL screen and makes it visible. Skip it and the Activity renders the bare default screen — there is no assertion or panic, just a blank display.

## Hold a Java field to any View that has a listener

Symptom: input works for a few seconds, then the keypad silently "loses focus," or a fresh screen throws `NoSuchMethod`.

The framework now roots listener-bound Views (key, touch, swipe, click, dialog, switch, checkbox, editor-action) as GC roots, so this no longer crashes on its own. But holding a Java field is the cleanest, most Android-idiomatic guard, and it removes any reliance on the native rooting — treat it as defense-in-depth.

```java
// FRAGILE: the only reference to this ListView lives in a native listener map.
@Override
protected void onCreate(Bundle savedInstanceState) {
  super.onCreate(savedInstanceState);
  ListView menu = new ListView();
  menu.setOnItemClickListener((parent, view, position, id) ->
      startActivity(new Intent(DESTINATIONS[position])));
  setContentView(menu);
}
```

```java
// BEST PRACTICE: keep a field so this Activity roots it too.
private ListView menu;

@Override
protected void onCreate(Bundle savedInstanceState) {
  super.onCreate(savedInstanceState);
  menu = new ListView();
  menu.setOnItemClickListener((parent, view, position, id) ->
      startActivity(new Intent(DESTINATIONS[position])));
  setContentView(menu);
}
```

Why: the GC is non-moving mark-sweep with slot reuse. A View reachable only through a Rust-side listener map (and not a Java field) was historically swept on the first GC; its heap slot was reused by another object, and a later dispatch resolved a live widget to a dead reference. The picoenvmon home hub keeps its menu `ListView` as a field redundantly for exactly this reason.

## No Handler, Looper, postDelayed, or Timer

Symptom: `Handler`, `Looper`, `postDelayed`, `java.lang.Thread` and `Timer` do not exist. `picodroid.concurrent.Thread` is the thread class (import it), with the `java.lang.Thread` API: `sleep`, `join`, `interrupt`, `currentThread`, a `Runnable` target or an overridden `run()`, and `Object.wait`/`notify`.

For background work, spawn a `Thread` and block on it — never on the main thread.

```java
// WRONG: none of these exist on Picodroid.
new Handler().postDelayed(this::sample, 1000);
java.lang.Thread.sleep(1000);
```

```java
// RIGHT: loop on a background Thread; hop results back to the UI.
new Thread(() -> {
  while (!Thread.currentThread().isInterrupted()) {
    final Reading r = sample();
    Executors.mainExecutor().execute(() -> label.setText(r.toString()));
    try {
      Thread.sleep(1000);
    } catch (InterruptedException e) {
      return;
    }
  }
}).start();
```

`Thread.sleep(long)` is interruptible and throws `InterruptedException` like Android's; `SystemClock.sleep(int)` sleeps through interrupts, also like Android's. To hop threads use `Executors.mainExecutor().execute(Runnable)` or `Executors.backgroundExecutor().execute(Runnable)` — `execute` runs as soon as the queue drains and has no delay overload.

For "do X in 500 ms" and "do X every second" use the JDK's `ScheduledExecutorService`, which is the delayed-work API here in place of `Handler.postDelayed` and `Timer`. `Executors.mainScheduledExecutor()` is the one whose tasks run on the main thread and may touch views:

```java
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.TimeUnit;

private final ScheduledExecutorService scheduler = Executors.mainScheduledExecutor();

scheduler.schedule(() -> status.setText("Saved"), 500, TimeUnit.MILLISECONDS);
scheduler.scheduleAtFixedRate(this::onTick, 1, 1, TimeUnit.SECONDS);

@Override
public void onDestroy() {
  scheduler.shutdownNow();   // or the tasks keep running against a dead Activity
  super.onDestroy();
}
```

Its single thread is the **main** thread: the frame tick posts each task when it is due, so a task may touch widgets and costs no thread stack, and a task that blocks stalls the UI. Deadlines are checked once per 16 ms frame, the runtime holds 16 scheduled tasks at a time, and nothing fires while the display is asleep. `testbench_rp2040` leaves the scheduled executor out of its framework (with `TimeUnit` and the `java.util.concurrent`-style pools), so on that board delayed work is still a `Thread` that sleeps and then posts. See [`examples/executordemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/executordemo).

For animation, use `view.animate()` ([ViewPropertyAnimator](/api/ui/)): `setDuration`, `setInterpolator`, and `withEndAction(Runnable)` for "do X after the animation" — the end action runs on the main thread.

Why: there is no Android main-loop `Handler`/`Looper` here; the Executors queue drains "sub-ms on Runnable post", and the scheduler is a deadline table on the frame tick rather than a thread. See [background services](/tutorials/background-service/).

## Button-only boards: widgets need setFocusable(true) + requestFocus()

Symptom: your `OnKeyListener` never fires on a hardware-button board.

A plain View is non-focusable by default (Android's default), and only a focusable view receives key events on a button device. `EditText` is the exception, focusable from its constructor as on Android.

```java
// WRONG: listener attached, but the view can't take focus, so keys never arrive.
LinearLayout panel = new LinearLayout();
panel.setOnKeyListener(this::onKey);
```

```java
// RIGHT: make it focusable and claim focus.
LinearLayout panel = new LinearLayout();
panel.setFocusable(true);
panel.setOnKeyListener(this::onKey);
panel.requestFocus();
```

Why: focusability is independent of `setOnKeyListener`, exactly as in Android. `requestFocus()` returns `false` without effect if the view is not focusable. ListView rows are focusable automatically (rendered as focusable list buttons), so adapter rows do not need this. A screen with nothing to focus (a dashboard, a clock face) needs no invisible focus-catcher: a key no focused view consumed reaches the foreground Activity's `onKeyDown` / `onKeyLongPress` / `onKeyUp`. Full input model: [button navigation](/guides/button-navigation/).

## Cap data-driven lists, and expect rebuilds to reset D-pad focus

Symptom: a long focusable list stalls the renderer on a small-pool board; rebuilding a `ListView` snaps keypad focus back to the top row.

On a board with a small LVGL memory pool (e.g. the 48 KB pool on `pico_enviro_mon`), keep focusable list rows short — the picoenvmon History screen caps at ~12. Each focusable row consumes render-pool memory; too many starve the LVGL draw tasks.

```java
// WRONG: dump the full 60-sample ring into focusable rows on a 48 KB-pool board.
for (Reading r : ring) {            // 60 rows
  adapter.add(r.toString());
}
list.setAdapter(adapter);
```

```java
// RIGHT: cap the window; e.g. show the most recent 12.
int start = Math.max(0, ring.size() - MAX_ROWS);   // MAX_ROWS = 12
for (int i = start; i < ring.size(); i++) {
  adapter.add(ring.get(i).toString());
}
list.setAdapter(adapter);
```

Why: this is an empirical, per-board limit driven by `lv_mem_kb`, not an API-enforced constant. Separately, `ListView.refreshFromAdapter()` removes and re-adds every row, so any live rebuild resets the highlighted row to position 0 — avoid rebuilding a focused list on a timer. See [the row-count limit](/reference/limits/) and [button navigation](/guides/button-navigation/).

## Stick to ASCII in rendered UI text

Symptom: an em-dash or ellipsis shows up as a `□` tofu box on screen.

Every bundled face is a Montserrat subset — the 14 px theme face and the larger sizes `TextView.setTextSize` can snap to — and none has the em-dash `—` (U+2014) or the ellipsis `…` (U+2026). The degree sign `°` (U+00B0) and the bullet `•` (U+2022) are present at every size.

```java
// WRONG: these codepoints render as tofu boxes.
status.setText("Connecting…");
reading.setText("temp — 21°C");
```

```java
// RIGHT: ASCII substitutes; ° is fine.
status.setText("Connecting...");
reading.setText("temp -- 21°C");
```

Why: the missing-glyph placeholder renders `□` for any codepoint outside the subset. Use `...` for an ellipsis and `--` for a dash. Encoding is UTF-8, and `°` is in the set — but adding new glyphs needs the font toolchain and costs flash. `TextView.setEllipsize` is safe: LVGL draws its ellipsis as three ASCII dots.

<a id="https-is-unsupported"></a>

## HTTPS needs a TLS board and a set clock

Symptom: an `https://` request throws `SSLHandshakeException: wall clock not set…` on a board that has just booted, or `UnsupportedOperationException` on a board built without TLS.

`https` URLs work on every board built with `has_tls = true` — the RP2350 WiFi boards. `URL.openConnection()` returns a `picodroid.net.ssl.HttpsURLConnection` and `connect()` runs a TLS 1.3 handshake that checks the certificate's validity against the wall clock. There is no battery-backed clock, so the runtime refuses to handshake until an app has set it.

```java
// WRONG: first request after boot, clock never set — SSLHandshakeException.
HttpURLConnection c = new URL("https://api.example.com/v1/x").openConnection();
c.connect();
```

```java
import javax.net.ssl.SSLHandshakeException;
import picodroid.net.HttpURLConnection;
import picodroid.net.SntpClient;
import picodroid.net.URL;
import picodroid.os.SystemClock;

// RIGHT: on a background thread, once the network is up — set the clock, then connect.
SntpClient ntp = new SntpClient();
if (ntp.requestTime("pool.ntp.org", 3000)) {
  SystemClock.setCurrentTimeMillis(
      ntp.getNtpTime() + SystemClock.elapsedRealtime() - ntp.getNtpTimeReference());
}
HttpURLConnection c = new URL("https://api.example.com/v1/x").openConnection();
c.setConnectTimeout(10000);
c.setReadTimeout(10000);
try {
  int status = c.getResponseCode();
} catch (SSLHandshakeException e) {   // javax.net.ssl; an IOException
  Log.w(TAG, "certificate rejected: " + e.getMessage());
} finally {
  c.disconnect();
}
```

Why, and what it costs:

- The chain is verified against a root store compiled into the firmware; an app cannot add a root or loosen the check, so a server under a private CA is refused (`not issued by a known root`).
- A handshake takes 1.1–2.0 s on the RP2350 (measured on `pico_display2_w`; an RSA chain is the slow end) and runs on a task of its own with a 40 KB stack taken from the heap arena for the length of the handshake. `connect()` blocks its caller meanwhile, so never call it on the main thread, and leave the arena room for that transient.
- A board built without `has_tls` throws `UnsupportedOperationException` from `connect()`. `testbench_rp2040` has neither TLS nor the networking classes.
- As before: only `GET`, `POST`, and `PUT` are supported — any other method throws `UnsupportedOperationException("method not supported: ...")`. For POST/PUT you must call `setDoOutput(true)` and `setFixedLengthStreamingMode(n)`, or `connect()` throws `IllegalStateException`. `Connection: close` is always sent (one connection per request). Timeouts default to infinite: set `setConnectTimeout` / `setReadTimeout`, which throw `SocketTimeoutException` on expiry.

See [HTTPS](/api/networking/#https).

## EditText is single-line, and supports a numeric (digit-pad) mode

Symptom: on a keypad board the X/ENTER that opens the soft keyboard used to insert a newline, making a field look cleared.

EditText is single-line by design and now enforces it natively, so ENTER no longer inserts a newline. For numeric fields, select the digit pad with `InputType.TYPE_CLASS_NUMBER`; for a password, a password variation (`InputType.TYPE_TEXT_VARIATION_PASSWORD`) masks the field with bullets, and `getText()` still returns the real text.

```java
// WRONG: expecting multi-line entry, and a text keyboard for a number field.
EditText interval = new EditText(this);
// (no input type set; user types digits via the full text layout)
```

```java
// RIGHT: digit-pad keyboard for numeric input.
EditText interval = new EditText(this);
interval.setInputType(InputType.TYPE_CLASS_NUMBER);
```

Why: `setInputType` mirrors `android.widget.TextView.setInputType`, but the class only picks the keyboard — `TYPE_CLASS_NUMBER` opens the digit pad; anything else uses the default text layout — and the password variations are the only ones that change the field. EditText is documented and enforced as one-line, so do not expect multi-line text entry. Pair numeric input with a tolerant parse (a stray value should fall back to a default), since the field carries exactly what the user typed.

## Idle sleep swallows the first wake keypress

Symptom: after the board idles to sleep, the first button press only wakes the screen — it does not navigate or click.

On every board, after the screen timeout (`idle_timeout_ms`, 60 s by default; Settings → Display can change it) with no key or touch, the display dozes while your app keeps running. The press or tap that wakes it is discarded so it never reaches LVGL focus nav, your `OnKeyListener` or a click listener. A second press is needed to actually act. This is by design, as on a phone, and the simulator does it too (its window goes black).

```java
// WRONG: assuming the first post-sleep press triggers your handler.
button.setOnKeyListener((v, event) -> { advance(); return true; });
```

```java
// RIGHT: nothing to change in code — just expect "first press wakes, second press acts".
// Keep handlers idempotent so a double-tap to wake-then-act is harmless.
```

Why: the wake drains the queued edges and the touch ring and tells LVGL to sit out the press. A screen that is watched rather than touched — a clock, a monitor — holds the panel on with `View.setKeepScreenOn(true)` on its root instead of fighting the timer; the board default can be tuned with `idle_timeout_ms` in `board.toml` (`0` never). While the display dozes only the render stops: `ScheduledExecutorService` tasks, alarms and callbacks still fire. See [system limits](/reference/limits/#display-idle-sleep).

## StringBuilder is byte-oriented and small

Symptom: a non-ASCII `char` appended to a `StringBuilder` comes out as one wrong byte, or a call such as `sb.insert(0, x)` is rejected by the build's API contract check.

Every `StringBuilder` owns its own buffer, so any number of builders can be open and interleaved. The surface is `append` (String, int, long, float, double, boolean, char, Object, `CharSequence`), `length`, `charAt` and `toString`.

```java
// WRONG: no insert / deleteCharAt / reverse / setLength.
sb.insert(0, prefix);

// RIGHT: build in order, or concatenate the pieces.
String s = prefix + sb.toString();
```

Why: the buffer holds bytes, not UTF-16 chars — `append(char)` emits a single byte (no multi-byte Unicode), and `charAt` returns the byte at that position. A buffer the heap cannot grow throws `OutOfMemoryError` from `append`.

## A bound-only Service dies when its Activity leaves

Symptom: a Service you only `bindService()` to resets its state every time you change screens.

A Service that is only bound — never started — is destroyed when its binding Activity finishes, taking its in-memory state with it. To keep data alive across screens, promote it to a started/foreground service.

```java
// WRONG: bind-only — the service's ring buffer is wiped on every screen change.
bindService(new Intent(this, SensorLoggerService.class), this, BIND_AUTO_CREATE);
```

```java
// RIGHT: start (and foreground) the service so it survives Activity changes.
Intent svc = new Intent(this, SensorLoggerService.class);
startService(svc);                           // promotes to started; survives the screen leave
bindService(svc, this, BIND_AUTO_CREATE);    // still bind to read its snapshot
```

Why: on Activity `finish()` the framework auto-unbinds that Activity's connections; if the service is neither started nor bound by anyone else, `onDestroy` runs immediately and its state is gone. A started (or foreground) service keeps running, so a later screen can bind the same instance and read accumulated data. See [background services](/tutorials/background-service/) and [the services API](/api/services/).

## A covered Activity can be destroyed and rebuilt

Symptom: the user comes back to a screen and its counters, selections or typed text are gone; or it only happens on the board, after several screens were opened.

An Activity covered by another normally keeps its instance and its hidden view tree. When a `startActivity` finds memory short — less than an eighth of the LVGL pool free, or less than a sixteenth of the heap — the Activities underneath are destroyed, oldest first, and each is re-created when the user returns to it: `onCreate(saved)`, `onStart`, `onRestoreInstanceState(saved)`, `onResume`, with no `onRestart`. Fields you did not save are gone.

```java
// WRONG: state lives only in a field.
private int count;
```

```java
// RIGHT: save it, and read it back in onCreate.
@Override
protected void onSaveInstanceState(Bundle outState) {
  outState.putInt("count", count);
}

@Override
protected void onCreate(Bundle savedInstanceState) {
  super.onCreate(savedInstanceState);
  count = savedInstanceState == null ? 0 : savedInstanceState.getInt("count");
}
```

Why: the default `onSaveInstanceState` saves nothing — not even an `EditText`'s text. Test with the equivalent of Android's *Don't keep activities*, which destroys every Activity the moment it is covered: `PICODROID_DONT_KEEP_ACTIVITIES=1 ./scripts/sim.sh --app myapp` (the same variable at build time bakes it into device firmware; the log says `activity: don't keep activities is ON`). A reclaim logs `activity: reclaim <class>` and the way back `activity: re-create <class> (was reclaimed)`. See [saved instance state](/api/ui/#saved-instance-state) and `examples/reclaimdemo`.

## A Fragment's views are dead after onDestroyView

Symptom: a Fragment that was replaced, detached or paged away updates a `TextView` it kept in a field, and nothing changes on screen.

```java
// WRONG: a background result lands after the page was turned.
label.setText(result);          // label belonged to the view given up in onDestroyView
```

```java
// RIGHT: drop the references with the view, and check before touching one.
@Override
public void onDestroyView() {
  label = null;
  super.onDestroyView();
}

if (label != null) label.setText(result);
```

Why: a panel cannot afford detached view trees, so the view a fragment gives up in `onDestroyView` is freed at once, its LVGL widgets with it, and the next `onCreateView` builds a fresh tree (`getView()` is `null` in between). A `ViewPager2` keeps one page alive at a time for the same reason. After the host Activity is destroyed and re-created, fragments come back only through a `FragmentFactory` set before `super.onCreate` — there is no reflection to instantiate them with. Fragments are on every RP2350 board; `testbench_rp2040` leaves the classes out. See [Fragments](/api/ui/#picodroidappfragment).

## Text sizes snap to the faces the board compiled

Symptom: `setTextSize(18)` and `setTextSize(22)` look identical, or every size looks the same on `testbench_rp2040`.

```java
title.setTextSize(24);                 // drawn in the 28 px face on an RP2350 board
int line = title.getLineHeight();      // tells you which face is in use
```

Why: the faces are bitmaps, so a size is drawn in the nearest compiled face, a tie going to the larger; `getTextSize()` still returns what you set. The ladder is the board's `text_sizes` key: `14;20;28;64` on the RP2350 boards and `14` alone on `testbench_rp2040`, where every size snaps to 14. `px`, `dp` and `sp` are all one pixel. Do not reach for `setScaleX` / `setScaleY` to get big text: scaling renders through an off-screen layer from the LVGL pool.

## onDraw records a bounded list of draw calls

Symptom: the last rows or shapes of a custom view's drawing are missing; the simulator prints `[sim] Canvas: onDraw recorded more than 2048 bytes of ops; N dropped` (on a device, `Canvas: onDraw overflowed its list; N ops dropped`).

Why: there is no pixel buffer behind a `Canvas`. Each draw call records a 32-byte op (a `drawText` adds its text bytes) into a per-view list of at most 2 KB in the LVGL pool — about 60 calls — and LVGL replays the list when it paints the view. Calls past the cap are dropped. Draw composite shapes with fewer primitives, or split a long drawing across several views. `onDraw` runs once per `invalidate()`, not once per frame, so call `invalidate()` (or `postInvalidate()` off the main thread) when the picture changes. `testbench_rp2040` has no `Canvas` (`has_canvas = false`). See [custom drawing](/api/ui/#custom-drawing-ondraw-canvas-and-paint).

## A full LVGL pool freezes the UI without a log line

Symptom: the screen stops updating and input is dead right after a screen built many widgets; the last log line is from before a widget was created, and no exception was thrown.

Widgets live in LVGL's own pool (`lv_mem_kb`, 48 KB on most boards), not in the Java heap, so `OutOfMemoryError` does not cover it: LVGL's allocation assert is compiled as an endless loop, and a widget the pool cannot hold spins the UI task there. Keep view trees small, `close()` or `removeView` what a screen no longer shows, cap data-driven lists (above), and remember that a covered Activity's hidden tree stays in the pool until memory pressure reclaims it.

To see how full the pool is, run a diagnostics build and read the `lv=used/total` field of the `[memmon]` line:

```bash
./scripts/sim.sh --app myapp --mem-diag
```

The simulator's pool is the board's `lv_mem_kb` × 1.6 (its pointers are twice as wide), so compare the ratio, not the bytes. See [`docs/memory-diagnostics.md`](https://github.com/shivrajora/picodroid-rs/blob/main/docs/memory-diagnostics.md).

## Other things to watch

- **`testbench_rp2040` runs a subset.** Its framework leaves out Fragments and `ViewPager2`, `Canvas` / `onDraw`, `java.time`, `picodroid.json`, `picodroid.protobuf`, `picodroid.media`, the scheduled executor and the `java.util.concurrent`-style pools, and all of `picodroid.net` except `NetworkInfo` (so no sockets, HTTP or TLS). Build with the board named — `./scripts/build-apk.sh --app myapp --board testbench_rp2040` — and the API contract check rejects an app that uses one of them at build time instead of on the device.
- **A stale widget handle is a no-op, not a crash.** A call on a view whose widget was already freed (`close()`, `removeView`, a Fragment's `onDestroyView`) resolves to nothing on a device: every handle carries the generation of its slot, so a stale one is detected instead of followed. The simulator's handle sanitizer, on by default, stops the run with a backtrace at the same call, which is where to fix it.
- **Containers are flat.** `LinearLayout`, `FrameLayout` and `RadioGroup` have no background, border, corner radius or padding until you set one, as on Android. Give a card its look with a `GradientDrawable` ([theming](/guides/theming/)).
- **A swipe goes to the nearest view with a listener.** A swipe that starts on a view, or on a child without a swipe listener of its own, reaches that view's `OnSwipeListener`; a child that has one keeps the swipe, as on Android. A pull-down inside a `SwipeRefreshLayout` goes to the layout first.
- **No reflection.** `Class.forName`, `newInstance`, and member discovery do not exist — `java.lang.Class` exposes only `getName()`. Code that loads classes by name will not compile; there is nothing for the shrinker to break reflectively.
- **`ArrayAdapter` needs a working `toString()`.** Rows render via `getItem(i).toString()`. Strings work directly; a custom item type must define `toString()` in Java or it throws `NoSuchMethod` when rendered.
- **Sensor registration cap is 8.** `getDefaultSensor(type)` returns `null` (and `registerListener` returns `false`) if the board has no matching sensor; call `unregisterListener()` in `onPause`/`onDestroy` to avoid leaking registration slots across app swaps.
- **Keep button hint legends short.** The hint bar is ~224 px wide; long legends clip. Use a short or single word per key.

## See also

- [Debugging](/guides/debugging/)
- [Troubleshooting](/guides/troubleshooting/)
- [System limits](/reference/limits/)
