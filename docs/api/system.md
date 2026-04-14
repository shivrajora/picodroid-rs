# System Services

Cross-cutting runtime services: logging, clocks, GC introspection, and threading. Packages: `picodroid.util`, `picodroid.os`, `picodroid.concurrent`. See [docs/README.md](../README.md) for the full API index.

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

## `picodroid.os.Runtime`

GC introspection. All methods are static.

```java
import picodroid.os.Runtime;

long nanos  = Runtime.gcTimeNanos();  // total time spent in GC so far (ns)
int  count  = Runtime.gcCount();      // number of GC cycles run
int  freed  = Runtime.gcFreed();      // total heap entries freed across all cycles
Runtime.resetGcStats();               // reset all three counters to zero
```

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

---

**See also:** [core.md](core.md) (Java language) · [peripherals.md](peripherals.md) (GPIO, UART, I2C, SPI, PWM, ADC) · [storage.md](storage.md) (files, preferences) · [networking.md](networking.md) (sockets) · [ui.md](ui.md) (display, widgets)
