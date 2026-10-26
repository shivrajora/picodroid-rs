---
title: "Tutorial: a background service bound from an Activity"
description: "Build an uptime-logging Service that survives navigation, then bind it from a viewer screen to read a live snapshot."
---

We'll build a small two-screen app whose real work happens off-screen. A `UptimeLogService` samples the device's monotonic uptime once a second into a ring buffer; a viewer Activity binds the service and lists the samples collected so far. Open the viewer twice and the list is longer the second time — because the service kept running while you were on the home screen.

The finished app is committed at [examples/tutorial_service](https://github.com/shivrajora/picodroid-rs/tree/main/examples/tutorial_service); every snippet below is copied from it. Run it any time with:

```bash
./scripts/sim.sh --app tutorial_service
```

The Service API is faithful to Android, so the patterns here transfer. For the per-method reference see [Services](/api/services/); for navigation mechanics see the [multi-screen tutorial](/tutorials/multi-screen-app/).

## The started-vs-bound lesson

This is the whole point of the app, so internalise it before writing code.

A **bound-only** service is destroyed the moment its last Activity finishes. When that screen pops, the framework auto-unbinds it, the bind count falls to zero, and — if nothing *started* the service — `onDestroy` runs immediately. Any in-memory state (here, our ring buffer) dies with it.

This bit real code. In picoenvmon, History always came up empty because the sensor logger was only ever **bound**: it was created when you opened a screen and destroyed when you left, so its ring buffer reset every time and the one-shot snapshot on connect always read zero. The fix was to also **start** the service. Once started, it kept sampling across screen changes; opening History then bound the *same running instance* and its snapshot returned the accumulated ring (`History bound, samples=60`).

So the recipe this tutorial follows:

1. **Start** the service from the entry screen, `HomeActivity`. Starting (not binding) is what keeps it alive across navigation.
2. **Bind** it from a screen only to read a snapshot, then unbind when that screen goes away.

The started service owns the data and the lifetime; binding is just a typed read handle.

## The Service

`UptimeLogService` extends `Service` and is both startable and bindable. Start the file with the binder.

```java
public class UptimeLogService extends Service {
  private static final String TAG = "UptimeLogService";
  private static final int NOTIFICATION_ID = 1;

  /** Fixed ring-buffer capacity. The newest {@code CAPACITY} samples are retained. */
  public static final int CAPACITY = 16;

  /** How often the background Thread takes a sample, in milliseconds. */
  private static final int SAMPLE_INTERVAL_MS = 1000;

  /**
   * Typed handle handed to clients: Android's LocalBinder pattern. Picodroid is single-process, so
   * the binder simply hands out the Service — clients cast the {@link IBinder} they receive back to
   * this type and call {@link #getService}.
   */
  public class LocalBinder extends Binder {
    public UptimeLogService getService() {
      return UptimeLogService.this;
    }
  }

  private final LocalBinder binder = new LocalBinder();
```

`IBinder` is the **interface** of the object `onBind` returns, and `Binder` is the class to extend for it, as on Android. The LocalBinder pattern is just a tiny `Binder` subclass that hands out the service. Picodroid is single-process, so there's no IPC: clients cast the `IBinder` they receive back to `LocalBinder` and call `getService()`.

`onCreate` runs exactly once — on the first start *or* the first bind, whichever happens first:

```java
@Override
public void onCreate() {
  // onCreate runs once, on the first start OR the first bind, whichever happens first.
  Log.i(TAG, "onCreate");
}
```

Now the ring buffer and the start machinery. The buffer is touched by two threads (the sampler writes, the main thread reads in `snapshot`), so every access goes through one `synchronized` block:

```java
// Circular buffer of monotonic timestamps (ms). head = next write index; count saturates at
// CAPACITY once the ring has wrapped. The sampler Thread writes them and the main thread reads
// them in snapshot(), so every access is inside a synchronized block on `lock` — this both
// publishes the writes across threads and makes each snapshot atomic with respect to a sample.
private final Object lock = new Object();
private final long[] samples = new long[CAPACITY];
private int head;
private int count;

// Guards onStartCommand so repeated startService calls don't spawn a second sampler Thread or a
// duplicate foreground banner — onStartCommand fires on EVERY startService, including repeats.
private boolean started;
private volatile boolean running;
```

`onStartCommand` has Android's three-argument shape — `(Intent intent, int flags, int startId)`. `flags` is always `0` (it reports redelivery after a process kill, which never happens on an MCU). It fires on **every** `startService`, including repeats, so guard the one-time setup with a `started` flag. Without that guard a second start would spawn a second sampler thread and a duplicate banner.

```java
@Override
public int onStartCommand(Intent intent, int flags, int startId) {
  Log.i(TAG, "onStartCommand id=" + startId);
  if (!started) {
    started = true;
    running = true;

    Notification n =
        new Notification.Builder(this, "uptime")
            .setContentTitle(getString(R.string.notification_title))
            .setContentText(getString(R.string.notification_text))
            .build();
    startForeground(NOTIFICATION_ID, n);

    // Sampling is blocking work (it sleeps between samples), so it MUST run off the main thread:
    // a Thread of its own, where SystemClock.sleep blocks nothing else.
    new Thread(this::sampleLoop).start();
    Log.i(TAG, "foreground started, sampler running");
  }
  // The return value is ignored on picodroid (the OS never kills a running Service); START_STICKY
  // is returned for source-level Android compatibility.
  return START_STICKY;
}
```

Three things to understand here:

- **`startForeground(id, Notification)`** promotes the service to foreground state and posts its notification (logged as `[notification 1] …`; the on-screen banner is [not drawn yet](/api/services/)). Call it from `onStartCommand` (after the service is registered), never from a constructor. `Notification.Builder(Context, String channelId)` is Android's constructor; the channel id is accepted and unused, since there are no channels.
- **`new Thread(this::sampleLoop).start()`** — a service runs on the main thread, and the main thread must never block. Sampling sleeps between samples, so it has to run off-thread. This is the ordinary `java.lang.Thread` (`start`, `join`, `interrupt`, `Thread.sleep`, names, an uncaught-exception handler) on a FreeRTOS task; see [Thread](/api/system/#javalangthread).
- **`return START_STICKY`** is for source-level Android compatibility only. On picodroid the OS never kills a running service, so the return value is **ignored**. Returning it keeps the code recognisable to Android developers; don't read meaning into it at runtime.

The loop runs on the background thread. `SystemClock.sleep(int)` is the sleep that cannot be interrupted and throws nothing, as on Android (`Thread.sleep` is the interruptible one) — safe here precisely because we are *not* on the main thread:

```java
private void sampleLoop() {
  while (running) {
    recordSample();
    SystemClock.sleep(SAMPLE_INTERVAL_MS);
  }
}

private void recordSample() {
  // elapsedRealtimeNanos() is monotonic (it never jumps backwards), which is what we want for an
  // uptime log; convert to milliseconds for a compact, readable value.
  long uptimeMs = SystemClock.elapsedRealtimeNanos() / 1_000_000L;
  synchronized (lock) {
    samples[head] = uptimeMs;
    head = (head + 1) % CAPACITY;
    if (count < CAPACITY) {
      count++;
    }
  }
}
```

`SystemClock.elapsedRealtimeNanos()` is monotonic — it never jumps backwards — which is exactly what an uptime log wants.

`onBind` returns the cached binder; binding to read state needs nothing more:

```java
@Override
public IBinder onBind(Intent intent) {
  Log.i(TAG, "onBind");
  return binder;
}

@Override
public boolean onUnbind(Intent intent) {
  Log.i(TAG, "onUnbind");
  return false;
}
```

`snapshot(long[])` copies the ring oldest-first under the **same** lock the sampler uses, so the caller always sees a consistent point-in-time view even mid-write. It returns how many samples it wrote:

```java
public int snapshot(long[] out) {
  synchronized (lock) {
    int n = count;
    int start = (head - n + CAPACITY) % CAPACITY;
    for (int i = 0; i < n && i < out.length; i++) {
      out[i] = samples[(start + i) % CAPACITY];
    }
    return n;
  }
}
```

Finally `onDestroy` stops the loop and clears the banner. Setting `running = false` lets the sampler thread fall out of its `while` on the next iteration:

```java
@Override
public void onDestroy() {
  // Stop the sampler loop, drop the foreground banner. onDestroy runs only when the Service is
  // neither started nor bound — for a started Service that means after stopService/stopSelf with
  // no remaining clients.
  running = false;
  Log.i(TAG, "onDestroy count=" + count);
  stopForeground(true);
}
```

## Starting it from the entry screen

The manifest declares every component the app starts, in Android's long form: the two screens and the Service. `HomeActivity` carries the `MAIN` / `LAUNCHER` intent filter, so it is what the launcher opens (and what boots first); there is no `Application` subclass. The Gradle `verifyManifest` task refuses an `Intent` that names an undeclared Service or Activity, which is why the `<service>` line is there ([manifest reference](/reference/manifest/#the-android-form)):

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="tutorial_service" version="1.0">
    <!-- Each component the app starts is declared: the two screens (the MAIN / LAUNCHER one is
         what the launcher opens) and the Service. The build refuses an Intent that names an
         undeclared Service or Activity. -->
    <application>
        <activity android:name=".HomeActivity">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
        <activity android:name=".LogViewerActivity" />
        <service android:name=".UptimeLogService" />
    </application>
</manifest>
```

`HomeActivity.onCreate` starts the service before it builds anything:

```java
@Override
protected void onCreate(Bundle savedInstanceState) {
  super.onCreate(savedInstanceState);
  Log.i(TAG, "onCreate");
  Log.i(TAG, "starting UptimeLogService");
  startService(new Intent(this, UptimeLogService.class));

  // The content view fills the window; no size needed.
  LinearLayout root = new LinearLayout(this);
  root.setOrientation(LinearLayout.VERTICAL);
  root.setPadding(10, 10, 10, 10);

  TextView title = new TextView(this);
  title.setText(getString(R.string.home_title));
  title.setTextColor(Color.WHITE);
  root.addView(title);

  TextView body = new TextView(this);
  body.setText(getString(R.string.home_body));
  body.setTextColor(Color.WHITE);
  root.addView(body);

  // A local, as on Android: the content view keeps the button and its listener alive.
  Button viewLogButton = new Button(this);
  viewLogButton.setText(getString(R.string.button_view_log));
  viewLogButton.setOnClickListener(v -> startActivity(new Intent(this, LogViewerActivity.class)));
  root.addView(viewLogButton, buttonParams());

  setContentView(root);
}
```

`startService` is the first thing Home does. A *started* service runs until something stops it, independent of the screen that started it, so it begins sampling on its background thread at once and keeps going as the user navigates; the viewer always finds an accumulated ring to read. `Intent` here is purely a target selector, Android's `new Intent(this, UptimeLogService.class)`. The texts come from `res/values/strings.xml` through `getString(R.string.…)` ([resources](/guides/resources/)), and `buttonParams()` is the one `LinearLayout.LayoutParams` every button in the app uses: full width, 8 px below the view above.

## The viewer screen

`LogViewerActivity` implements `ServiceConnection` directly, so the Activity *is* its own connection callback. It declares the adapter and the status line as fields, since Refresh rewrites them:

```java
public class LogViewerActivity extends Activity implements ServiceConnection {
  private static final String TAG = "LogViewerActivity";

  // The adapter and the status line are fields because Refresh rewrites them; the list and the
  // button are locals — the tree keeps them and the listener alive, as on Android.
  private ArrayAdapter<String> adapter;
  private TextView statusLine;

  // The bound Service handle, captured in onServiceConnected. null until connected and after
  // disconnect; Refresh guards on it.
  private UptimeLogService service;

  private final long[] samples = new long[UptimeLogService.CAPACITY];
```

`onCreate` builds the title, the status line, a list over an `ArrayAdapter` with the framework's one-line row (`picodroid.R.layout.simple_list_item_1`, as `android.R.layout` on Android) taking the height that is left (weight 1), and a Refresh button, and binds at the end. `bindService` takes Android's `(Intent, ServiceConnection, int flags)`, with `BIND_AUTO_CREATE`:

```java
@Override
protected void onCreate(Bundle savedInstanceState) {
  super.onCreate(savedInstanceState);
  Log.i(TAG, "onCreate");

  LinearLayout root = new LinearLayout(this);
  root.setOrientation(LinearLayout.VERTICAL);
  root.setPadding(10, 10, 10, 10);

  TextView title = new TextView(this);
  title.setText(getString(R.string.log_title));
  title.setTextColor(Color.WHITE);
  root.addView(title);

  statusLine = new TextView(this);
  statusLine.setText(getString(R.string.log_connecting));
  statusLine.setTextColor(Color.CYAN);
  root.addView(statusLine);

  // The list takes whatever height the title, status and button leave (weight 1).
  adapter = new ArrayAdapter<>(this, picodroid.R.layout.simple_list_item_1);
  ListView list = new ListView(this);
  list.setAdapter(adapter);
  root.addView(list, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

  Button refreshButton = new Button(this);
  refreshButton.setText(getString(R.string.button_refresh));
  refreshButton.setOnClickListener(v -> refresh());
  root.addView(refreshButton, HomeActivity.buttonParams());

  setContentView(root);

  // onServiceConnected delivers the binder between frames; onDestroy unbinds.
  Log.i(TAG, "bindService");
  bindService(new Intent(this, UptimeLogService.class), this, BIND_AUTO_CREATE);
}
```

`onServiceConnected` is delivered between frames, with Android's two arguments: the `ComponentName` of the service and its `IBinder`. Cast the binder back to the `LocalBinder`, grab the typed handle, and do an initial read:

```java
@Override
public void onServiceConnected(ComponentName name, IBinder binder) {
  service = ((UptimeLogService.LocalBinder) binder).getService();
  Log.i(TAG, "onServiceConnected");
  refresh();
}

@Override
public void onServiceDisconnected(ComponentName name) {
  Log.i(TAG, "onServiceDisconnected");
  service = null;
}
```

`onServiceDisconnected` is Android's "the service's process died" callback; it is not called for your own `unbindService`, and Picodroid runs every service in the app's own process, so it never fires here. Null the handle in it all the same, so the code ports.

`refresh()` re-reads the snapshot into the reusable `samples` array and refills the adapter (`clear`, `add`, `notifyDataSetChanged`), which re-binds the list's rows in place. It's wired to the Refresh button and also called once on connect. Guard on `service == null` so a tap before the connection lands is a harmless no-op:

```java
/** Re-read the Service's snapshot and refill the list. Wired to the Refresh button. */
private void refresh() {
  if (service == null) {
    Log.i(TAG, "refresh skipped (not connected)");
    return;
  }
  int n = service.snapshot(samples);
  Log.i(TAG, "refresh, samples=" + n);

  if (n == 0) {
    statusLine.setText(getString(R.string.log_empty));
  } else {
    statusLine.setText(getString(R.string.log_count, n));
  }

  adapter.clear();
  for (int i = 0; i < n; i++) {
    adapter.add(getString(R.string.log_row, i, samples[i]));
  }
  adapter.notifyDataSetChanged();
}
```

Unbind in `onDestroy`, as on Android. The framework **also** unbinds, silently, any connection an Activity still owns when it finishes, so a forgotten unbind leaks nothing; the explicit call is what Android requires and keeps the code portable:

```java
@Override
public void onDestroy() {
  super.onDestroy();
  Log.i(TAG, "onDestroy, unbindService");
  unbindService(this);
}
```

Crucially, unbinding the viewer does **not** stop the service — it's still *started* (by `HomeActivity`), so it keeps sampling.

## Lifecycle: read the logs

Run it and watch the boot sequence. The service comes up before the first screen:

```bash
./scripts/sim.sh --app tutorial_service
```

```text
[HomeActivity] onCreate
[HomeActivity] starting UptimeLogService
[HomeActivity] onResume
[UptimeLogService] onCreate
[UptimeLogService] onStartCommand id=1
[notification 1] Uptime Logger: Recording uptime samples
[UptimeLogService] foreground started, sampler running
```

Home builds and resumes first; the start is delivered right after, on the main thread: the service's `onCreate`, then `onStartCommand id=1` (the start id begins at 1 and increments per call), then the foreground banner and the sampler. The service is collecting samples while Home is on screen.

The viewer has no Back button of its own, and the simulator's default board (`testbench_rp2350`: touch, no buttons) has no BACK key. Simulate a board with both to walk the rest — Backspace on the host keyboard is its BACK button:

```bash
./scripts/sim.sh --board pico_touch_kit --app tutorial_service
```

Tap **View Log** and the viewer binds the already-running service:

```text
[LogViewerActivity] onCreate
[LogViewerActivity] bindService
[LogViewerActivity] onResume
[UptimeLogService] onBind
[LogViewerActivity] onServiceConnected
[LogViewerActivity] refresh, samples=N
```

`onBind` fires once for this instance; `onServiceConnected` delivers the binder; the connect-time `refresh` reads `samples=N`. Press BACK to return to Home (this auto-unbinds), wait, then open the viewer again — `N` is **larger**, because the started service never stopped sampling. That growing count is the started-vs-bound lesson made visible: a bound-only service would have reset to zero on every visit.

## When does the service actually stop?

`onDestroy` runs exactly when the service is **neither started nor bound**. For this app that condition is never met while the app runs — Home started it and nothing stops it — so it survives every navigation between its screens and only stops at app exit, when the framework tears down all live services and runs their `onDestroy`.

App exit is an ordinary event on a board with a launcher: BACK on Home finishes the last Activity, which ends the app and brings the launcher back, and so does the HOME key or starting another app. One app runs at a time, so the service does not keep sampling behind the launcher — its ring buffer starts empty the next time the app is started. See the [launcher guide](/guides/launcher/).

If you wanted to end it sooner, you'd call `stopService(new Intent(context, UptimeLogService.class))` from anywhere with a `Context`, or `stopSelf()` from inside the service. Either drops the *started* state; once no clients are bound either, `onDestroy` runs and the banner clears. Until then, binding and unbinding viewers just raises and lowers the bind count — the service and its ring buffer keep going.

Next steps: the [Services reference](/api/services/) for the full method surface, the [multi-screen tutorial](/tutorials/multi-screen-app/) for navigation, and the [examples index](/examples/) for more committed apps.
