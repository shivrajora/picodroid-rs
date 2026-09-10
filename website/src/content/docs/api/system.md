---
title: "System Services"
description: "Log, SystemClock, Runtime, Thread, and the main / background Executors."
---

Cross-cutting runtime services: logging, clocks, GC introspection, threading, and executors. Packages: `picodroid.util`, `picodroid.os`, `picodroid.concurrent`. See [Java API overview](/api/) for the full API index.

## `picodroid.util.Log`

```java
import picodroid.util.Log;

Log.i("TAG", "message");   // info log → defmt::info! over RTT
```

## `picodroid.os.SystemClock`

```java
import picodroid.os.SystemClock;

SystemClock.sleep(500);               // sleep for 500 ms
long t = SystemClock.elapsedRealtimeNanos();  // nanoseconds since boot (monotonic)
```

## `java.lang.System.currentTimeMillis()`

Convenience for the common Android idiom `long now = System.currentTimeMillis();`. Returns milliseconds elapsed since boot — there is no wall-clock RTC on the Pico, so the value is monotonic but not Unix-epoch-relative. Equivalent to `SystemClock.elapsedRealtimeNanos() / 1_000_000`.

```java
long start = System.currentTimeMillis();
doWork();
long elapsed = System.currentTimeMillis() - start;
```

See [`examples/clockdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/clockdemo).

## `picodroid.os.Runtime`

GC and heap introspection. All methods are static.

```java
import picodroid.os.Runtime;

long nanos  = Runtime.gcTimeNanos();  // total time spent in GC so far (ns)
int  count  = Runtime.gcCount();      // number of GC cycles run
int  freed  = Runtime.gcFreed();      // total heap entries freed across all cycles
Runtime.resetGcStats();               // reset all three counters to zero

long used = Runtime.usedMemory();     // current heap usage (bytes)
long peak = Runtime.peakMemory();     // high-water heap usage so far (bytes)
Runtime.resetPeakMemory();            // reset the peak counter to the current usage
```

`usedMemory` / `peakMemory` / `resetPeakMemory` are handy for profiling — bracket a workload with
`resetPeakMemory()` then read `peakMemory()` to capture its high-water allocation.

## `picodroid.os.Build`

What the firmware was built for, as `android.os.Build` reports it:

```java
Build.BOARD            // the board.toml name, e.g. "testbench_rp2350"
Build.HARDWARE         // the MCU, e.g. "rp2350"
Build.VERSION.RELEASE  // the firmware release, e.g. "0.23.0"
```

`VERSION.RELEASE` is the firmware's version — the one a shrink map is cut for, so on a `--shrink` image it is also the framework map version `pdb ping` reports, the one a PAPK's own map version must be compatible with to install. A plain (unshrunk) build reports its release here too, while `pdb ping` shows the `0.0.0` sentinel that means "no map".

## `picodroid.os.StatFs`

Space on the storage volume — see [storage](/api/storage/#picodroidosstatfs).

## `picodroid.content.pm.PackageManager`

What the device has installed, and what it can run. `Context.getPackageManager()` returns it (every Activity, Application and Service is a Context). Mirrors `android.content.pm.PackageManager`.

```java
import picodroid.content.pm.ApplicationInfo;
import picodroid.content.pm.PackageInfo;
import picodroid.content.pm.PackageManager;

PackageManager pm = getPackageManager();
boolean wifi = pm.hasSystemFeature(PackageManager.FEATURE_WIFI);

List<PackageInfo> apps = pm.getInstalledPackages(0);               // system apps included
PackageInfo info = pm.getPackageInfo("com.example.weather", 0);    // NameNotFoundException when absent
CharSequence label = pm.getApplicationLabel(info.applicationInfo);
Drawable icon = pm.getApplicationIcon(info.applicationInfo);       // null when the app has no icon
Intent launch = pm.getLaunchIntentForPackage("com.example.weather"); // null when absent
startActivity(launch);                                             // ends this app, starts that one
```

| Method | Description |
|--------|-------------|
| `hasSystemFeature(String)` | `FEATURE_WIFI`, `FEATURE_ETHERNET`: the board's link, a build fact. |
| `getInstalledPackages(int flags)` | Every package, system apps included. `flags` is ignored. |
| `getPackageInfo(String, int flags)` | One package, or `PackageManager.NameNotFoundException`. |
| `getLaunchIntentForPackage(String)` | An Intent that starts the package, or `null`. |
| `getApplicationLabel(ApplicationInfo)` | The manifest `label`, or the package name. |
| `getApplicationIcon(String)` / `(ApplicationInfo)` | The manifest `icon` as a `BitmapDrawable`, or `null`. |

`PackageInfo` carries `packageName`, `versionName`, `versionCode` and `applicationInfo`. `ApplicationInfo` carries `packageName` and `flags` (`FLAG_SYSTEM` for an app built into the firmware, such as the launcher) and can `loadLabel(pm)` and `loadIcon(pm)`.

The query methods exist on multi-app boards only (`max_installed_apps` above 1). A single-app board keeps `hasSystemFeature` and drops the rest, `PackageInfo`, `ApplicationInfo` and `BitmapDrawable` included. See the [launcher guide](/guides/launcher/).

`Context.getPackageName()` returns the running app's own package name, from its manifest.

The package directory keeps each entry's name, label, version and icon name as it scanned them, so every query above is a few native calls and no manifest is parsed on the way; `getApplicationIcon` still looks the icon up in the package's asset table. Building the rows that show the result is the expensive part on a device — see the [launcher guide](/guides/launcher/#costs).

## `picodroid.concurrent.Thread`

The `java.lang.Thread` API on a FreeRTOS task. Import it — there is no `java.lang.Thread` here.

```java
import picodroid.concurrent.Thread;

Thread t = new Thread(new MyRunnable(), "worker");
t.start();            // spawns a FreeRTOS task that calls MyRunnable.run()
t.join();             // or join(ms); InterruptedException if this thread is interrupted
t.isAlive();          // false once run() has returned

Thread.currentThread().getName();   // "main" on the UI thread
Thread.sleep(250);                  // interruptible; throws InterruptedException
t.interrupt();                      // wakes sleep/join/wait on t, or sets its flag

// Subclass form, uncaught-exception handler
Thread u = new Thread("u") { @Override public void run() { /* ... */ } };
Thread.setDefaultUncaughtExceptionHandler((th, e) -> Log.e("app", th.getName() + ": " + e));

// Monitors: synchronized blocks AND methods lock; Object.wait/notify work
synchronized (lock) { while (!ready) lock.wait(); }
```

Second `start()` throws `IllegalThreadStateException`; `wait`/`notify` outside `synchronized` throw `IllegalMonitorStateException`. `SystemClock.sleep(int)` is the non-interruptible sleep, as on Android.

### Complete Runnable example

```java
import picodroid.concurrent.Thread;
import picodroid.util.Log;
import picodroid.os.SystemClock;

public class MyApp {
    public static void main(String[] args) {
        Thread worker = new Thread(new Runnable() {
            public void run() {
                for (int i = 0; i < 3; i++) {
                    Log.i("Worker", "tick " + String.valueOf(i));
                    SystemClock.sleep(1000);
                }
            }
        });
        worker.setPriority(Thread.MAX_PRIORITY);
        worker.start();

        Log.i("Main", "Worker started, main continues");
    }
}
```

### Priority

`Thread.MIN_PRIORITY` (1), `NORM_PRIORITY` (5) and `MAX_PRIORITY` (10) exist and `setPriority`/`getPriority` round-trip them, but the value is **advisory**: every task that interprets Java — the UI thread, every `Thread`, the background executor pool — runs at the single JVM tier (FreeRTOS priority 15), below real-time native tasks (21–30) and above background native services (1–10). The shared JVM heap is lock-free on the strength of "a running JVM task keeps the core until it blocks", and a Java thread one notch above the UI thread would preempt it at any instruction. Android itself only treats `setPriority` as a scheduling hint; here the hint is recorded and not applied.

Each call to `t.start()` creates a dedicated FreeRTOS task with a 4096-word stack. When `MyRunnable.run()` returns, the task self-deletes and its stack is reclaimed automatically.

All JVM child threads are pinned to **core 0**, the same core as the `jvm` task. This keeps the single-core safety assumption of `SharedJvmState` intact — no JVM state is ever accessed from core 1.

On hot-swap, any thread blocked inside `SystemClock.sleep()`, `Thread.sleep()`, `join()` or `Object.wait()` is woken immediately so it can see the stop signal and exit cleanly before the new app starts.

For fire-and-forget work, prefer [`Executors.backgroundExecutor()`](#picodroidconcurrentexecutors) over spawning a dedicated `Thread`: the pool amortises stack allocation across jobs and keeps per-task overhead bounded.

## `picodroid.concurrent.Executors`

Android-style `java.util.concurrent.Executor` bindings for posting Runnables onto the framework's own threads. Two executors are exposed:

- **Main-thread executor** — runs Runnables on the JVM task's main loop, interleaved with LVGL ticks on a 16 ms frame budget. Use this to touch widgets or any other state that must only be read/written from the main thread.
- **Background pool** — a fixed-size FreeRTOS thread pool with its own worker tasks. Use this for short blocking work (I/O, sensor reads, crypto) that you don't want to stall the UI.

```java
import picodroid.concurrent.Executor;
import picodroid.concurrent.Executors;
import picodroid.util.Log;

Executor main = Executors.mainExecutor();
Executor bg   = Executors.backgroundExecutor();

bg.execute(() -> {
    String result = fetchSomethingSlow();
    main.execute(() -> label.setText(result));  // hop back to the UI thread
});
```

The `Executor` interface is a single method:

```java
public interface Executor {
    void execute(Runnable command);
}
```

`execute()` is non-blocking and returns immediately. If the target queue is full, the Runnable is **dropped** with a `defmt::warn` and no exception — plan for occasional backpressure rather than relying on every post to land. The main queue has capacity 64; the background queue's depth is configurable per board.

### Background pool configuration

The pool is tuned via a `[background_pool]` section in [`board.toml`](/reference/porting-guide/#boardtoml-reference). All keys optional:

```toml
[background_pool]
threads      = 4       # 1..=32 (default 4)
priority     = 5       # 1..=10 FreeRTOS BG tier (default 5)
stack_bytes  = 4096    # per-worker stack (default 4 KiB)
queue_depth  = 32      # shared job queue depth (default 32)
```

Each worker owns its own `Jvm` instance, so Runnables posted to the background pool run with a **separate** JVM state from the main loop. Treat any shared object references as if they crossed a thread boundary.

See [`examples/executordemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/executordemo) for a worked example.

---

**See also:** [Core language](/api/core/) · [Services & DI](/api/services/) · [Peripherals](/api/peripherals/) · [Storage](/api/storage/) · [Networking](/api/networking/) · [Sensors](/api/sensors/) · [Graphics & UI](/api/ui/)
