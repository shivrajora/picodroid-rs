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

## `picodroid.concurrent.Thread`

```java
import picodroid.concurrent.Thread;

Thread t = new Thread(new MyRunnable());
t.start();   // spawns a FreeRTOS task that calls MyRunnable.run()

// Priority (optional, must be set before start())
t.setPriority(Thread.MAX_PRIORITY);  // 1 (MIN) .. 5 (NORM, default) .. 10 (MAX)
int p = t.getPriority();
```

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

| Java constant | Value | FreeRTOS priority |
|---|---|---|
| `Thread.MIN_PRIORITY` | 1 | 11 |
| `Thread.NORM_PRIORITY` | 5 | 15 (default) |
| `Thread.MAX_PRIORITY` | 10 | 20 |

Priorities follow the Android `Thread` API (1–10). Internally they map to FreeRTOS priorities 11–20 (the JVM tier), which sit below real-time native tasks (21–30) and above background native services (1–10). `setPriority` must be called before `start()`; changing priority on a running thread is not supported.

Each call to `t.start()` creates a dedicated FreeRTOS task with a 4096-word stack. When `MyRunnable.run()` returns, the task self-deletes and its stack is reclaimed automatically.

All JVM child threads are pinned to **core 0**, the same core as the `jvm` task. This keeps the single-core safety assumption of `SharedJvmState` intact — no JVM state is ever accessed from core 1.

On hot-swap, any thread blocked inside `SystemClock.sleep()` is woken immediately so it can see the stop signal and exit cleanly before the new app starts.

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
