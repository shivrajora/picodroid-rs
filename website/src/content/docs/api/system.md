---
title: "System Services"
description: "Log, SystemClock, Runtime, Build, PackageManager, Thread and monitors, the main / background Executors, ScheduledExecutorService, and the java.util.concurrent core set."
---

Cross-cutting runtime services: logging, clocks, GC introspection, threading, and executors. Packages: `picodroid.util`, `picodroid.os`, `picodroid.content.pm`, `picodroid.concurrent`. See [Java API overview](/api/) for the full API index.

## `picodroid.util.Log`

```java
import picodroid.util.Log;

Log.v("TAG", "verbose");
Log.d("TAG", "debug");
Log.i("TAG", "message");   // info log → defmt::info! over RTT
Log.w("TAG", "warning");
Log.e("TAG", "error");
Log.e("TAG", "fetch failed", exception);   // appends ": " + exception.getMessage()
```

`android.util.Log`'s severity ladder — `v`, `d`, `i`, `w`, `e` and `wtf` — each as `(String tag, String msg)` and `(String tag, String msg, Throwable tr)`. On hardware the level maps to the matching defmt severity, so an RTT viewer can filter on it; the simulator prints every level in the same `[Tag] message` form. `wtf` logs at error severity. The `Throwable` overloads append the throwable's message, not a stack trace.

## `picodroid.os.SystemClock`

```java
import picodroid.os.SystemClock;

SystemClock.sleep(500);               // sleep for 500 ms
long t = SystemClock.elapsedRealtimeNanos();  // nanoseconds since boot (monotonic)
long ms = SystemClock.elapsedRealtime();      // the same clock in milliseconds
SystemClock.setCurrentTimeMillis(epochMillis);  // anchor the wall clock; always returns true
```

| Method | Description |
|--------|-------------|
| `static void sleep(int ms)` | Sleep; not interruptible (`Thread.sleep` is). |
| `static long elapsedRealtimeNanos()` | Nanoseconds since boot. |
| `static long elapsedRealtime()` | Milliseconds since boot. It never jumps — setting the wall clock leaves it alone — which makes it the right base for a delay. |
| `static boolean setCurrentTimeMillis(long millis)` | Anchors the wall clock, after which `System.currentTimeMillis()` returns epoch time. Typically fed from an SNTP sync ([`SntpClient`](/api/networking/#wall-clock-sntpclient)). Always `true`: Android's permission-denied case does not apply. |

## `java.lang.System.currentTimeMillis()`

The wall clock. There is no battery-backed RTC on the Pico, so the value counts milliseconds from boot until something calls `SystemClock.setCurrentTimeMillis`; from then on it is Unix-epoch time. Setting the clock makes the value jump, so measure intervals that must survive a clock sync with `SystemClock.elapsedRealtime()` instead.

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
Build.VERSION.RELEASE  // the firmware release, e.g. "0.35.0"
```

`VERSION.RELEASE` is the firmware's version — the one a shrink map is cut for, so on a `--shrink` image it is also the framework map version `pdb ping` reports, the one a PAPK's own map version must be compatible with to install. A plain (unshrunk) build reports its release here too, while `pdb ping` shows the `0.0.0` sentinel that means "no map".

## `picodroid.os.StatFs`

Space on the storage volume — see [storage](/api/storage/#picodroidosstatfs).

## `picodroid.os.Bundle`

The typed map behind Intent extras and saved instance state — see [Graphics & UI](/api/ui/#picodroidosbundle).

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

`PackageInfo` carries `packageName`, `versionName`, `versionCode` (also as `getLongVersionCode()`; 1 when the manifest sets none) and `applicationInfo`. `ApplicationInfo` carries `packageName` and `flags` (`FLAG_SYSTEM` for an app built into the firmware, such as the launcher) and can `loadLabel(pm)` and `loadIcon(pm)`.

The query methods exist on multi-app boards only (`max_installed_apps` above 1). A single-app board keeps `hasSystemFeature` and drops the rest, `PackageInfo`, `ApplicationInfo` and `BitmapDrawable` included. See the [launcher guide](/guides/launcher/).

`Context.getPackageName()` returns the running app's own package name, from its manifest.

### `picodroid.content.pm.PackageInstaller`

The uninstall half of Android's `PackageInstaller`, from `getPackageManager().getPackageInstaller()`. Multi-app boards only.

```java
import picodroid.content.pm.PackageInstaller;

PackageInstaller installer = getPackageManager().getPackageInstaller();
installer.uninstall("com.example.weather");   // returns when the app and its data are gone
```

`uninstall(String packageName)` is synchronous — Android reports the outcome through an `IntentSender` — and the app that calls it keeps running. It throws `IllegalArgumentException` when the package is not installed, is a system app, or is the app making the call, and `IllegalStateException` when the device could not erase it. The package's `/data/<package>` directory goes with it (see [storage](/api/storage/#the-reserve-and-the-cap)).

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

| Member | Description |
|--------|-------------|
| `Thread()`, `Thread(Runnable target)`, `Thread(String name)`, `Thread(Runnable target, String name)` | Pass a target or override `run()`. An unnamed thread is `Thread-N`. |
| `void start()` | Runs `run()` on a task of its own. `IllegalThreadStateException` on a second call; `OutOfMemoryError` when the task cannot be created. |
| `static Thread currentThread()` | The calling task's `Thread`; the UI task (named `main`) and the executor workers get one the first time they ask. |
| `static void sleep(long millis)`, `sleep(long millis, int nanos)` | Interruptible; any `nanos` above zero rounds up to the next millisecond. |
| `final void join()`, `join(long millis)` | Wait for the thread to finish; `0` waits forever. |
| `void interrupt()`, `boolean isInterrupted()`, `static boolean interrupted()` | A blocked `sleep`, `join` or `wait` throws `InterruptedException`; otherwise the flag stays set until read. `interrupted()` clears it. |
| `final boolean isAlive()` | `false` once `run()` has returned. |
| `static void yield()` | Let another ready Java task run. |
| `getName()` / `setName(String)`, `getId()`, `toString()` | `toString()` is `Thread[name,priority]`. |
| `getPriority()` / `setPriority(int)`, `isDaemon()` / `setDaemon(boolean)` | Recorded and reported, never applied — see [Priority](#priority). `setDaemon` after `start()` throws `IllegalThreadStateException`. |
| `setUncaughtExceptionHandler` / `getUncaughtExceptionHandler`, `static setDefaultUncaughtExceptionHandler` / `getDefaultUncaughtExceptionHandler` | `Thread.UncaughtExceptionHandler.uncaughtException(Thread t, Throwable e)` receives whatever escapes `run()`. With no handler the exception is logged at error level. |

### Monitors: `synchronized`, `wait` and `notify`

`synchronized` blocks and `synchronized` methods both lock, and a monitor is reentrant. `Object.wait()`, `wait(long millis)`, `notify()` and `notifyAll()` work on any object, under Java's rules: the caller must own the monitor (`IllegalMonitorStateException` otherwise), a wait is interruptible, and a wait may wake spuriously, so wait in a loop that re-checks its condition.

```java
synchronized (queue) {
    while (queue.isEmpty()) {
        queue.wait();          // releases the monitor while parked
    }
    item = queue.remove(0);
}

synchronized (queue) {
    queue.add(item);
    queue.notify();            // wakes the longest-waiting thread
}
```

See [`examples/syncdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/syncdemo) and [`examples/threadparity/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/threadparity).

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

`Thread.MIN_PRIORITY` (1), `NORM_PRIORITY` (5) and `MAX_PRIORITY` (10) exist and `setPriority`/`getPriority` round-trip them, but the value is **advisory**: every task that interprets Java — the UI thread, every `Thread`, the background executor pool — runs at the single JVM tier (FreeRTOS priority 15), below real-time native tasks (21–30) and above background native services (1–10). The shared JVM heap has no locks of its own: one task interprets Java at a time, holding a kernel mutex (the JVM run lock) that it gives up only at a blocking point — a sleep, a wait, a queue, a socket, `Thread.yield`. A Java thread one notch above the UI thread would preempt it at any instruction, only to block on that mutex. Android itself only treats `setPriority` as a scheduling hint; here the hint is recorded and not applied.

Each call to `t.start()` creates a dedicated FreeRTOS task with a 4096-word (16 KiB) stack on the RP2350 and a 2048-word (8 KiB) stack on the RP2040. When `MyRunnable.run()` returns, the task self-deletes and its stack is reclaimed automatically. At most 16 Java threads are alive at once; a `start()` past that, or one the heap has no stack for, throws `OutOfMemoryError`.

All JVM child threads are pinned to **core 0**, the same core as the `jvm` task. This keeps the single-core safety assumption of `SharedJvmState` intact — no JVM state is ever accessed from core 1.

On hot-swap, any thread blocked inside `SystemClock.sleep()`, `Thread.sleep()`, `join()` or `Object.wait()` is woken immediately so it can see the stop signal and exit cleanly before the new app starts.

For fire-and-forget work, prefer [`Executors.backgroundExecutor()`](#picodroidconcurrentexecutors) over spawning a dedicated `Thread`: the pool amortises stack allocation across jobs and keeps per-task overhead bounded.

## `picodroid.concurrent.Executors`

Android-style `java.util.concurrent.Executor` bindings for posting Runnables onto the framework's own threads. Two executors are built in:

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

| Factory | Returns |
|---------|---------|
| `static Executor mainExecutor()` | The UI thread's executor. |
| `static Executor backgroundExecutor()` | The framework's shared background pool. |
| `static ScheduledExecutorService newSingleThreadScheduledExecutor()` | Delayed and periodic tasks on the main thread — [below](#delayed-and-periodic-work-scheduledexecutorservice). |
| `static ExecutorService newFixedThreadPool(int nThreads)` | A pool of your own — see [`ExecutorService`](#executorservice-future-and-callable). |
| `static ExecutorService newSingleThreadExecutor()` | One worker; tasks run strictly in order. |

### Delayed and periodic work: `ScheduledExecutorService`

There is no `Handler.postDelayed` and no `Timer`. Delayed and periodic work goes through `java.util.concurrent`'s shape instead, on every RP2350 board (the `testbench_rp2040` image leaves the scheduler out with the rest of the executors, `framework_class_excludes`):

```java
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.TimeUnit;

ScheduledExecutorService scheduler = Executors.newSingleThreadScheduledExecutor();

scheduler.schedule(() -> toast.cancel(), 2, TimeUnit.SECONDS);            // once
ScheduledFuture<?> clock =
    scheduler.scheduleAtFixedRate(this::tick, 1, 1, TimeUnit.SECONDS);    // every second
scheduler.scheduleWithFixedDelay(this::poll, 0, 30, TimeUnit.SECONDS);   // 30 s after each run ends

clock.cancel(false);        // one task
scheduler.shutdownNow();    // all of them — call it from onDestroy
```

`schedule` (a `Runnable` or a `Callable<V>`), `scheduleAtFixedRate`, `scheduleWithFixedDelay`, `execute`, `submit`, `shutdown`, `shutdownNow`, `isShutdown`, `isTerminated` and `awaitTermination` carry their JDK signatures and semantics, with one difference the name does not say: the executor's *single thread is the main thread*. The runtime keeps a table of sixteen deadlines that the 16 ms frame tick checks, and posts each due task to the main queue, so a scheduled task costs one table slot rather than a 16 KiB thread stack, and it may touch widgets directly. The price is the same as a `Handler`'s: a task that blocks stalls the UI while it runs, so hand blocking work to `backgroundExecutor()` from inside the task. Two consequences of the tick-driven design: a task runs within a frame of its due time when the main thread is idle (later if a Runnable ahead of it is slow), and nothing fires while the display is in low-power sleep.

A fixed-rate task keeps its schedule through a late run, but a run so late that the next due time has already passed is followed by one a full period later rather than by a burst of catch-up runs. A task that throws is logged, its future completes exceptionally, and if it was periodic it is not run again. `shutdown()` keeps the JDK's default policy — one-shot tasks still due run, periodic ones stop; `shutdownNow()` cancels everything. When the table is full, scheduling throws `RejectedExecutionException`, as it does after a shutdown. The sixteen slots are shared by every scheduler in the app.

A `ScheduledFuture<V>` is a `Future<V>` with `getDelay(TimeUnit)`: the time until the next run, zero or negative once due. A periodic task's future never completes normally — only when the task throws or is cancelled. Delays are measured on `SystemClock.elapsedRealtime()`, so setting the wall clock does not move them; a period or delay of zero or less throws `IllegalArgumentException`.

### Background pool configuration

The pool is tuned via a `[background_pool]` section in [`board.toml`](/reference/porting-guide/#boardtoml-reference). All keys optional:

```toml
[background_pool]
threads      = 4       # 1..=32 (default 4)
priority     = 15      # must be 15, the JVM tier (default 15)
stack_bytes  = 4096    # per-worker stack (default 4 KiB; the RP2350 boards set 6144 or 8192)
queue_depth  = 32      # shared job queue depth (default 32)
```

`priority` exists for completeness: the pool runs Java, every task that runs Java shares one priority, and the build fails on any other value. The workers run against the same classes and the same heap as the main thread, one Java task at a time, so an object a Runnable shares with the UI thread really is shared — guard it with `synchronized` or one of the atomics below.

See [`examples/executordemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/executordemo) for a worked example.

## The `java.util.concurrent` core set

`picodroid.concurrent` carries the part of `java.util.concurrent` apps reach for first, under the JDK's class names and signatures. It is written in Java over `Thread`, `synchronized` and `wait` / `notify`, with no native code behind it. The exceptions keep their `java.util.concurrent` package so `throws` and `catch` clauses compile unchanged: `ExecutionException`, `CancellationException`, `TimeoutException`, `RejectedExecutionException`.

Every RP2350 board has these classes. The `testbench_rp2040` image leaves them out (`framework_class_excludes`) and keeps `Thread`, `Executor` and the two built-in executors.

### `ExecutorService`, `Future` and `Callable`

```java
import java.util.concurrent.ExecutionException;
import picodroid.concurrent.ExecutorService;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Future;
import picodroid.concurrent.TimeUnit;

ExecutorService pool = Executors.newFixedThreadPool(2);

Future<Integer> answer = pool.submit(() -> compute());   // a Callable<Integer>
try {
    int v = answer.get(500, TimeUnit.MILLISECONDS);      // TimeoutException when not ready
} catch (ExecutionException e) {
    Throwable cause = e.getCause();                      // what compute() threw
}

pool.shutdown();                                         // queued tasks still run
pool.awaitTermination(1, TimeUnit.SECONDS);
```

| Member | Description |
|--------|-------------|
| `void execute(Runnable command)` | Queue a task. `RejectedExecutionException` after a shutdown. The queue is unbounded. |
| `Future<?> submit(Runnable task)`, `<T> Future<T> submit(Callable<T> task)` | Queue a task and get its `Future`. |
| `void shutdown()` | Stop accepting tasks; queued tasks still run. |
| `List<Runnable> shutdownNow()` | Stop accepting tasks, interrupt the workers, return the tasks that never started. |
| `boolean isShutdown()`, `boolean isTerminated()` | Terminated is shut down with every worker exited. |
| `boolean awaitTermination(long timeout, TimeUnit unit)` | `false` if the timeout elapsed first. |
| `Future.get()`, `get(long timeout, TimeUnit unit)` | The result. `ExecutionException` wraps what the task threw, `CancellationException` for a cancelled task, `TimeoutException` from the timed form. |
| `Future.cancel(boolean mayInterruptIfRunning)`, `isCancelled()`, `isDone()` | `cancel` is `false` for a task already completed or cancelled. A running task is not stopped; its result is discarded. |
| `Callable<V>` | `V call() throws Exception`. |
| `FutureTask<V>` | `FutureTask(Callable<V>)`, `FutureTask(Runnable, V result)`: a `Runnable` and a `Future` in one, for running a computation on a thread of your own. |

Each worker of a pool is a `Thread`, and costs a thread's stack for as long as the pool lives. For occasional work prefer `backgroundExecutor()`, whose workers already exist. A task that throws from `execute` is logged and the worker carries on. There is no `invokeAll` / `invokeAny`, no cached pool and no `CompletableFuture`.

### `TimeUnit`

`NANOSECONDS`, `MICROSECONDS`, `MILLISECONDS`, `SECONDS`, `MINUTES`, `HOURS`, `DAYS`, with `convert(long sourceDuration, TimeUnit sourceUnit)`, `toNanos`, `toMicros`, `toMillis`, `toSeconds`, `toMinutes` and `sleep(long timeout)`.

### Atomics

`AtomicInteger`, `AtomicLong`, `AtomicBoolean` and `AtomicReference<V>`.

```java
import picodroid.concurrent.AtomicInteger;

AtomicInteger hits = new AtomicInteger();      // or new AtomicInteger(10)
int n = hits.incrementAndGet();
boolean swapped = hits.compareAndSet(1, 100);
```

| Class | Methods |
|-------|---------|
| All four | `get()`, `set(v)`, `lazySet(v)`, `getAndSet(v)`, `compareAndSet(expect, update)`, `toString()` |
| `AtomicInteger`, `AtomicLong` | `getAndIncrement()`, `getAndDecrement()`, `getAndAdd(delta)`, `incrementAndGet()`, `decrementAndGet()`, `addAndGet(delta)`, `intValue()`, `longValue()` |

Atomicity comes from `synchronized`: one Java task runs at a time, so a monitor is the cheapest correct primitive. `AtomicReference.compareAndSet` compares by identity. There is no `updateAndGet` or `accumulateAndGet`, and no atomic arrays.

### `CountDownLatch`

```java
import picodroid.concurrent.CountDownLatch;

CountDownLatch done = new CountDownLatch(3);
// each worker, when finished:
done.countDown();
// the thread that waits for all three:
boolean all = done.await(2, TimeUnit.SECONDS);   // false on timeout; await() waits forever
```

`CountDownLatch(int count)` (`IllegalArgumentException` for a negative count), `countDown()`, `await()`, `await(long timeout, TimeUnit unit)` and `getCount()`. There is no `Semaphore`, `CyclicBarrier`, `ReentrantLock`, `ConcurrentHashMap` or `BlockingQueue`; a `synchronized` block over an `ArrayList` with `wait` / `notify` is the queue.

See [`examples/jucdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/jucdemo).

---

**See also:** [Core language](/api/core/) · [Services & DI](/api/services/) · [Peripherals](/api/peripherals/) · [Storage](/api/storage/) · [Networking](/api/networking/) · [Sensors](/api/sensors/) · [Graphics & UI](/api/ui/)
