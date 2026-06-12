---
title: "Embedded gotchas: writing robust apps"
description: "The Android idioms that behave differently on Picodroid hardware, and the right pattern for each."
---

Picodroid keeps the `android.*` API surface, but the runtime is a Rust JVM on an MCU with a few hundred KB of RAM, no reflection, and a hardware-button input model. The patterns below are the ones an Android developer reaches for by reflex that misbehave here — each one lists the symptom, the wrong and right code, and why.

## BACK on the root Activity exits the app

Symptom: pressing BACK (the Y button) on your home screen quits the whole app instead of doing nothing.

The default `Activity.onBackPressed()` calls `finish()`, which pops the Activity off the stack. On the root Activity that is the last entry, so the app exits — the standard Android launcher behavior. Your top-level Activity must override it.

```java
// WRONG: no override — BACK on the root pops the last entry and exits the app.
public class HomeActivity extends Activity { /* ... */ }
```

```java
// RIGHT: swallow BACK on the root; deliberately do NOT call super.
@Override
public void onBackPressed() {
  // no-op
}
```

Why: `finish()` triggers `onPause -> onStop -> onDestroy`, and when it empties the stack the app shuts down. Only the root needs this; pushed Activities should keep the default so BACK returns to the parent. See [button navigation](/guides/button-navigation/).

## setContentView() is mandatory or the screen is blank

Symptom: the Activity runs, no exception is thrown, but the display shows nothing.

```java
// WRONG: onCreate builds a tree but never installs it.
@Override
public void onCreate() {
  LinearLayout root = new LinearLayout(this);
  root.addView(new TextView(this));
  // ...screen stays blank.
}
```

```java
// RIGHT: install the root view.
@Override
public void onCreate() {
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
public void onCreate() {
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
public void onCreate() {
  menu = new ListView();
  menu.setOnItemClickListener((parent, view, position, id) ->
      startActivity(new Intent(DESTINATIONS[position])));
  setContentView(menu);
}
```

Why: the GC is non-moving mark-sweep with slot reuse. A View reachable only through a Rust-side listener map (and not a Java field) was historically swept on the first GC; its heap slot was reused by another object, and a later dispatch resolved a live widget to a dead reference. The picoenvmon home hub keeps its menu `ListView` as a field redundantly for exactly this reason.

## No Handler, Looper, postDelayed, Thread.sleep, or Timer

Symptom: `Handler`, `Looper`, `postDelayed`, `java.lang.Thread`, and `Timer` do not exist; `Thread.sleep` is absent even on `picodroid.concurrent.Thread`.

For background work, spawn a `picodroid.concurrent.Thread` and block with `SystemClock.sleep` — never on the main thread.

```java
// WRONG: none of these classes/methods exist on Picodroid.
new Handler().postDelayed(this::sample, 1000);
Thread.sleep(1000);
```

```java
// RIGHT: loop on a background Thread; hop results back to the UI.
new Thread(() -> {
  while (running) {
    final Reading r = sample();
    Executors.mainExecutor().execute(() -> label.setText(r.toString()));
    SystemClock.sleep(1000);
  }
}).start();
```

`SystemClock.sleep(int ms)` is the only blocking sleep in the SDK. To hop threads use `Executors.mainExecutor().execute(Runnable)` or `Executors.backgroundExecutor().execute(Runnable)` — `execute` runs as soon as the queue drains; there is no delay or timer overload.

For animation, use `view.animate()` ([ViewPropertyAnimator](/api/ui/)). Note the v1 caveats: linear interpolation only, **no completion listener**, and both endpoints are required.

```java
// "do X after the animation" — fire synchronously after start(); there is no callback.
view.animate().alpha(1.0f, 0.0f).x(0, 120).setDuration(300).start();
onAnimationKickedOff();
```

Why: there is no Android main-loop `Handler`/`Looper` here. The Executors queue drains "sub-ms on Runnable post," and the animation engine plays in the background without a finish callback. See [background services](/tutorials/background-service/).

## Button-only boards: widgets need setFocusable(true) + requestFocus()

Symptom: your `OnKeyListener` never fires on a hardware-button board.

A plain View is non-focusable by default (Android's default), and only a focusable view receives key events on a button device.

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

Why: focusability is independent of `setOnKeyListener`, exactly as in Android. `requestFocus()` returns `false` without effect if the view is not focusable. ListView rows are focusable automatically (rendered as focusable list buttons), so adapter rows do not need this. Full input model: [button navigation](/guides/button-navigation/).

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

The only bundled font is LVGL Montserrat 14, and its glyph subset has neither the em-dash `—` (U+2014) nor the ellipsis `…` (U+2026). The degree sign `°` (U+00B0) is present.

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

Why: the missing-glyph placeholder renders `□` for any codepoint outside the subset. Use `...` for an ellipsis and `--` for a dash. Encoding is UTF-8, and `°` is in the set — but adding new glyphs needs the font toolchain and costs flash.

## HTTPS is unsupported

Symptom: opening an `https://` connection throws at `connect()` time.

```java
// WRONG: HTTPS URL — throws UnsupportedOperationException("HTTPS not yet supported").
HttpURLConnection c = new URL("https://api.example.com").openConnection();
c.connect();
```

```java
// RIGHT: plain HTTP, with GET / POST / PUT only.
HttpURLConnection c = new URL("http://api.example.com").openConnection();
c.setRequestMethod("GET");
c.connect();
```

Why: `connect()` rejects the `https` protocol with `java.lang.UnsupportedOperationException` and the message `"HTTPS not yet supported"`. Only `GET`, `POST`, and `PUT` are supported — any other method throws `UnsupportedOperationException("method not supported: ...")`. For POST/PUT you must call `setDoOutput(true)` and `setFixedLengthStreamingMode(n)`, or `connect()` throws `IllegalStateException`. There is no per-operation timeout, and `Connection: close` is always sent (one connection per request); a hang is usually a DNS failure.

## EditText is single-line, and supports a numeric (digit-pad) mode

Symptom: on a keypad board the X/ENTER that opens the soft keyboard used to insert a newline, making a field look cleared.

EditText is single-line by design and now enforces it natively, so ENTER no longer inserts a newline. For numeric fields, select the digit pad with `InputType.TYPE_CLASS_NUMBER`.

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

Why: `setInputType` mirrors `android.widget.TextView.setInputType`, but only the class is honored — `TYPE_CLASS_NUMBER` opens the digit pad; anything else uses the default text layout. EditText is documented and enforced as one-line, so do not expect multi-line text entry. Pair numeric input with a tolerant parse (a stray value should fall back to a default), since the field carries exactly what the user typed.

## Idle sleep swallows the first wake keypress

Symptom: after the board idles to sleep, the first button press only wakes the screen — it does not navigate or click.

On a button board, after `idle_timeout_ms` of no input the display sleeps. The press that wakes it (and its release edge) is discarded so it never reaches LVGL focus nav or your `OnKeyListener`. A second press is needed to actually act. This is by design, and does not apply to the simulator or to touch-only boards.

```java
// WRONG: assuming the first post-sleep press triggers your handler.
button.setOnKeyListener((v, event) -> { advance(); return true; });
```

```java
// RIGHT: nothing to change in code — just expect "first press wakes, second press acts".
// Keep handlers idempotent so a double-tap to wake-then-act is harmless.
```

Why: the wake path drains both edges of the wake press before resuming the tick source. Tune or disable the timeout via `idle_timeout_ms` in `board.toml` (default 60000 ms, `0` disables). See [system limits](/reference/limits/).

## StringBuilder is a LIFO stack — finish builders in reverse order

Symptom: appends land on the wrong builder when two are open at once.

StringBuilder is implemented natively as a stack of buffers, not per-instance. You may **nest** a builder inside an unfinished one, but you must not **interleave** two live builders: finish (call `toString()` on) builders in reverse order of creation.

```java
// WRONG: two builders open at once; the older append lands on the wrong (top) buffer.
StringBuilder outer = new StringBuilder();
outer.append("a=");
StringBuilder inner = new StringBuilder();   // pushes a new top buffer
outer.append(x);                             // BUG: appends to inner's buffer, not outer's
inner.append(y);
String s = inner.toString();
String t = outer.toString();
```

```java
// RIGHT: fully consume the inner builder before resuming the outer (strict nesting).
StringBuilder outer = new StringBuilder();
outer.append("a=");
StringBuilder inner = new StringBuilder();
inner.append(y);
String innerStr = inner.toString();          // pops inner; outer is the top again
outer.append(x).append(innerStr);
String t = outer.toString();
```

Why: all `append`/`length`/`charAt` operate on the top of the buffer stack; `<init>` pushes, `toString()` pops. Nesting is fine because the inner is pushed and popped within the outer's lifetime; interleaving is not. Also note `append(char)` emits a single byte (no multi-byte Unicode), `append(float)`/`append(double)` format to ≤6 significant digits, and there is no `insert`, `deleteCharAt`, `reverse`, or `setLength`.

## A bound-only Service dies when its Activity leaves

Symptom: a Service you only `bindService()` to resets its state every time you change screens.

A Service that is only bound — never started — is destroyed when its binding Activity finishes, taking its in-memory state with it. To keep data alive across screens, promote it to a started/foreground service.

```java
// WRONG: bind-only — the service's ring buffer is wiped on every screen change.
bindService(new Intent(SensorLoggerService.class), this);
```

```java
// RIGHT: start (and foreground) the service so it survives Activity changes.
Intent svc = new Intent(SensorLoggerService.class);
startService(svc);                 // promotes to started; survives the screen leave
bindService(svc, this);            // still bind to read its snapshot
```

Why: on Activity `finish()` the framework auto-unbinds that Activity's connections; if the service is neither started nor bound by anyone else, `onDestroy` runs immediately and its state is gone. A started (or foreground) service keeps running, so a later screen can bind the same instance and read accumulated data. See [background services](/tutorials/background-service/) and [the services API](/api/services/).

## Other things to watch

- **No reflection.** `Class.forName`, `newInstance`, and member discovery do not exist — `java.lang.Class` exposes only `getName()`. Code that loads classes by name will not compile; there is nothing for the shrinker to break reflectively.
- **`ArrayAdapter` needs a working `toString()`.** Rows render via `getItem(i).toString()`. Strings work directly; a custom item type must define `toString()` in Java or it throws `NoSuchMethod` when rendered.
- **Sensor registration cap is 8.** `getDefaultSensor(type)` returns `null` (and `registerListener` returns `false`) if the board has no matching sensor; call `unregisterListener()` in `onPause`/`onDestroy` to avoid leaking registration slots across app swaps.
- **Keep button hint legends short.** The hint bar is ~224 px wide; long legends clip. Use a short or single word per key.

## See also

- [Debugging](/guides/debugging/)
- [Troubleshooting](/guides/troubleshooting/)
- [System limits](/reference/limits/)
