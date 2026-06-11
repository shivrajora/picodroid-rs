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

1. **Start** the service from the Application. Starting (not binding) is what keeps it alive across navigation.
2. **Bind** it from a screen only to read a snapshot, then unbind when that screen goes away.

The started service owns the data and the lifetime; binding is just a typed read handle.

## The Service

`UptimeLogService` extends `Service` and is both startable and bindable. Start the file with the binder.

```java
public class UptimeLogService extends Service {
  private static final String TAG = "UptimeLogService";
  private static final int NOTIFICATION_ID = 1;

  /** Fixed ring-buffer capacity. The newest CAPACITY samples are retained. */
  public static final int CAPACITY = 16;

  /** How often the background Thread takes a sample, in milliseconds. */
  private static final int SAMPLE_INTERVAL_MS = 1000;

  public static class LocalBinder implements IBinder {
    public UptimeLogService service;
  }

  private final LocalBinder binder = new LocalBinder();
```

`IBinder` is an **interface** — a marker for the object `onBind` returns. The LocalBinder pattern is just a tiny class that `implements IBinder` and carries a direct reference back to the service. Picodroid is single-process, so there's no IPC: clients cast the `IBinder` they receive back to `LocalBinder` and read `.service`. Note `implements IBinder`, not `extends` — `IBinder` is not a class.

Wire the binder to the live instance in `onCreate`, which runs exactly once — on the first start *or* the first bind, whichever happens first:

```java
@Override
public void onCreate() {
  binder.service = this;
  Log.i(TAG, "onCreate");
}
```

Now the ring buffer and the start machinery. The buffer is touched by two threads (the sampler writes, the main thread reads in `snapshot`), so every access goes through one `synchronized` block:

```java
private final Object lock = new Object();
private final long[] samples = new long[CAPACITY];
private int head;
private int count;

private boolean started;
private volatile boolean running;
```

`onStartCommand` is the **two-argument** form — `(Intent intent, int startId)`, no `flags`. It fires on **every** `startService`, including repeats, so guard the one-time setup with a `started` flag. Without that guard a second start would spawn a second sampler thread and a duplicate banner.

```java
@Override
public int onStartCommand(Intent intent, int startId) {
  Log.i(TAG, "onStartCommand id=" + startId);
  if (!started) {
    started = true;
    running = true;

    Notification n =
        new Notification.Builder()
            .setContentTitle("Uptime Logger")
            .setContentText("Recording uptime samples")
            .build();
    startForeground(NOTIFICATION_ID, n);

    new Thread(this::sampleLoop).start();
    Log.i(TAG, "foreground started, sampler running");
  }
  return START_STICKY;
}
```

Three things to understand here:

- **`startForeground(id, Notification)`** promotes the service to foreground state and shows a persistent top-of-screen banner. Call it from `onStartCommand` (after the service is registered), never from a constructor.
- **`new Thread(this::sampleLoop).start()`** — a service runs on the main thread, and the main thread must never block. Sampling sleeps between samples, so it has to run off-thread. This is `picodroid.concurrent.Thread`: its API is just `new Thread(Runnable)` and `start()`. It has **no** `sleep`, `join`, or `interrupt`.
- **`return START_STICKY`** is for source-level Android compatibility only. On picodroid the OS never kills a running service, so the return value is **ignored**. Returning it keeps the code recognisable to Android developers; don't read meaning into it at runtime.

The loop runs on the background thread. `SystemClock.sleep(int)` is the only blocking sleep in the SDK — safe here precisely because we are *not* on the main thread:

```java
private void sampleLoop() {
  while (running) {
    recordSample();
    SystemClock.sleep(SAMPLE_INTERVAL_MS);
  }
}

private void recordSample() {
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
  running = false;
  Log.i(TAG, "onDestroy count=" + count);
  stopForeground(true);
}
```

## Wiring it from the Application

The Application is the app's entry point — the manifest names it (`<application application="tutorial_service/TutorialServiceApp" />`) and the framework calls `onCreate` once at boot. Two calls, and the order is the whole trick:

```java
public class TutorialServiceApp extends Application {
  @Override
  public void onCreate() {
    Log.i("TutorialServiceApp", "starting UptimeLogService");
    startService(new Intent(UptimeLogService.class));
    startActivity(new Intent(HomeActivity.class));
  }
}
```

`startService` **before** `startActivity`. Starting the service from the Application — rather than from an Activity — is what makes it outlive any single screen. It begins sampling on its background thread immediately and keeps going as the user navigates, so the viewer always finds an accumulated ring to read. Start it from an Activity instead and it would only live as long as that Activity's stack.

`Intent` here is purely a target selector: `new Intent(UptimeLogService.class)`. There's no `(Context, Class)` constructor — just the class.

## The viewer screen

`LogViewerActivity` implements `ServiceConnection` directly, so the Activity *is* its own connection callback. It builds a list and a Refresh button, then binds in `onCreate`:

```java
public class LogViewerActivity extends Activity implements ServiceConnection {
  private ListView list;
  private TextView statusLine;
  private Button refreshButton;

  private UptimeLogService service;
  private final long[] samples = new long[UptimeLogService.CAPACITY];
```

Hold the listener-bearing views in fields, and bind at the end of `onCreate`. `bindService` is **two-argument** — `(Intent, ServiceConnection)` — with no flags and no `BIND_AUTO_CREATE`:

```java
  list = new ListView();
  list.setSize(200, 130);
  root.addView(list);

  refreshButton = new Button("Refresh");
  refreshButton.setSize(200, 36);
  refreshButton.setOnClickListener(v -> refresh());
  root.addView(refreshButton);

  setContentView(root);

  Log.i(TAG, "bindService");
  bindService(new Intent(UptimeLogService.class), this);
}
```

`onServiceConnected` is delivered between frames, with a **single** `IBinder` argument (no `ComponentName`). Cast it back to the `LocalBinder`, grab the typed handle, and do an initial read:

```java
@Override
public void onServiceConnected(IBinder binder) {
  service = ((UptimeLogService.LocalBinder) binder).service;
  Log.i(TAG, "onServiceConnected");
  refresh();
}

@Override
public void onServiceDisconnected() {
  Log.i(TAG, "onServiceDisconnected");
  service = null;
}
```

`onServiceDisconnected` takes **no** arguments. It fires when the service goes away (last unbind, the owning Activity destroyed, or app exit) — null the handle so nothing calls back into a dead reference.

`refresh()` re-reads the snapshot into the reusable `samples` array and rebuilds the list. It's wired to the Refresh button and also called once on connect. Guard on `service == null` so a tap before the connection lands is a harmless no-op:

```java
private void refresh() {
  if (service == null) {
    Log.i(TAG, "refresh skipped (not connected)");
    return;
  }
  int n = service.snapshot(samples);
  Log.i(TAG, "refresh, samples=" + n);

  if (n == 0) {
    statusLine.setText("No samples yet");
  } else {
    statusLine.setText(n + " samples (ms)");
  }

  ArrayAdapter<String> adapter = new ArrayAdapter<String>();
  for (int i = 0; i < n; i++) {
    adapter.add("[" + i + "] " + samples[i] + " ms");
  }
  list.setAdapter(adapter);
}
```

Unbind in `onDestroy`. The framework **also** auto-unbinds any connection an Activity owns when it finishes, so this is technically redundant — but explicit unbinding is good form, and it's wrapped in a `try`/`catch` so a double-unbind can't fault the lifecycle:

```java
@Override
public void onDestroy() {
  Log.i(TAG, "onDestroy, unbindService");
  try {
    unbindService(this);
  } catch (Throwable t) {
    Log.i(TAG, "unbind ignored: " + t);
  }
}
```

Crucially, unbinding the viewer does **not** stop the service — it's still *started* from the Application, so it keeps sampling.

## Lifecycle: read the logs

Run it and watch the boot sequence. The service comes up before the first screen:

```bash
./scripts/sim.sh --app tutorial_service
```

```text
[TutorialServiceApp] starting UptimeLogService
[UptimeLogService] onCreate
[UptimeLogService] onStartCommand id=1
[UptimeLogService] foreground started, sampler running
[HomeActivity] onCreate
```

`onCreate` then `onStartCommand id=1` (the start id begins at 1 and increments per call), the foreground banner and sampler come up, *then* HomeActivity builds. The service is already collecting samples while Home is on screen.

Tap **View Log** and the viewer binds the already-running service:

```text
[LogViewerActivity] onCreate
[LogViewerActivity] bindService
[UptimeLogService] onBind
[LogViewerActivity] onServiceConnected
[LogViewerActivity] refresh, samples=N
```

`onBind` fires once for this instance; `onServiceConnected` delivers the binder; the connect-time `refresh` reads `samples=N`. Press BACK to return to Home (this auto-unbinds), wait, then open the viewer again — `N` is **larger**, because the started service never stopped sampling. That growing count is the started-vs-bound lesson made visible: a bound-only service would have reset to zero on every visit.

## When does the service actually stop?

`onDestroy` runs exactly when the service is **neither started nor bound**. For this app that condition is never met during normal use — it was started from the Application and stays started — so it survives every navigation and only stops at app exit, when the framework tears down all live services and runs their `onDestroy`.

If you wanted to end it sooner, you'd call `stopService(new Intent(UptimeLogService.class))` from anywhere with a `Context`, or `stopSelf()` from inside the service. Either drops the *started* state; once no clients are bound either, `onDestroy` runs and the banner clears. Until then, binding and unbinding viewers just raises and lowers the bind count — the service and its ring buffer keep going.

Next steps: the [Services reference](/api/services/) for the full method surface, the [multi-screen tutorial](/tutorials/multi-screen-app/) for navigation, and the [examples index](/examples/) for more committed apps.
