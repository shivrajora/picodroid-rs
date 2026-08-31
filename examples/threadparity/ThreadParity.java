// SPDX-License-Identifier: GPL-3.0-only
package threadparity;

import picodroid.app.Application;
import picodroid.concurrent.Thread;
import picodroid.util.Log;

/**
 * End-to-end checks of {@code java.lang.Thread} semantics on {@code picodroid.concurrent.Thread}:
 * synchronized methods under real contention, join and timed join, interrupting a sleeper, start()
 * twice, a subclass overriding run(), wait/notify producer-consumer, timed wait, wait outside
 * synchronized, the uncaught-exception handler, the interrupt flag, and a start/join churn. Logs
 * one PASS/FAIL line per check and {@code === PASSED ===} when all hold.
 */
public class ThreadParity extends Application {
  private static final String TAG = "ThreadParity";
  private static int fails = 0;

  private static void check(String what, boolean ok) {
    Log.i(TAG, (ok ? "PASS: " : "FAIL: ") + what);
    if (!ok) {
      fails = fails + 1;
    }
  }

  // A counter guarded by synchronized *methods* — the form the interpreter
  // ignored before ACC_SYNCHRONIZED support.
  private int counter = 0;

  synchronized void inc() {
    counter = counter + 1;
  }

  synchronized int get() {
    return counter;
  }

  // Producer/consumer state guarded by `lock` with wait/notify.
  private final Object lock = new Object();
  private int item = -1;
  private int consumed = 0;

  @Override
  public void onCreate() {
    Log.i(TAG, "=== ThreadParity start ===");
    try {
      runChecks();
    } catch (Throwable e) {
      check("no exception escaped the checks: " + e, false);
    }
    if (fails == 0) {
      Log.i(TAG, "=== PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED (" + fails + ") ===");
    }
  }

  private void runChecks() throws InterruptedException {
    // 1. The main task has a Thread of its own.
    Thread main = Thread.currentThread();
    check("main thread is named main", "main".equals(main.getName()));
    check("main thread is alive", main.isAlive());
    check("currentThread() is stable", Thread.currentThread() == main);

    // 2. Synchronized methods under contention, then join.
    Thread a = new Thread(() -> loopInc(5000), "inc-a");
    Thread b = new Thread(() -> loopInc(5000), "inc-b");
    a.start();
    b.start();
    a.join();
    b.join();
    check("synchronized method serialised 10000 increments: " + get(), get() == 10000);
    check("joined threads are no longer alive", !a.isAlive() && !b.isAlive());
    check("thread names round-trip", "inc-a".equals(a.getName()) && "inc-b".equals(b.getName()));

    // 3. start() twice.
    boolean threw = false;
    try {
      a.start();
    } catch (IllegalThreadStateException e) {
      threw = true;
    }
    check("start() twice throws IllegalThreadStateException", threw);

    // 4. A subclass overriding run(), and currentThread() inside it.
    final String[] seen = new String[1];
    Thread sub =
        new Thread("sub") {
          @Override
          public void run() {
            seen[0] = Thread.currentThread().getName();
          }
        };
    sub.start();
    sub.join();
    check("subclass run() ran on its own thread", "sub".equals(seen[0]));

    // 5. interrupt() wakes Thread.sleep.
    final boolean[] interrupted = new boolean[1];
    Thread sleeper =
        new Thread(
            () -> {
              try {
                Thread.sleep(10000);
              } catch (InterruptedException e) {
                interrupted[0] = true;
              }
            },
            "sleeper");
    sleeper.start();
    Thread.sleep(50);
    sleeper.interrupt();
    sleeper.join(2000);
    check("interrupt() wakes Thread.sleep with InterruptedException", interrupted[0]);
    check("interrupted sleeper finished", !sleeper.isAlive());

    // 6. Timed join returns early; plain join waits.
    Thread slow = new Thread(() -> quietSleep(400), "slow");
    slow.start();
    long t0 = System.currentTimeMillis();
    slow.join(50);
    long dt = System.currentTimeMillis() - t0;
    check(
        "join(50) returned while the target still ran (" + dt + " ms)", slow.isAlive() && dt < 300);
    slow.join();
    check("join() waited for the end", !slow.isAlive());

    // 7. wait/notify producer-consumer.
    Thread consumer = new Thread(this::consume, "consumer");
    consumer.start();
    for (int i = 0; i < 10; i++) {
      synchronized (lock) {
        while (item >= 0) {
          lock.wait();
        }
        item = i;
        lock.notifyAll();
      }
    }
    consumer.join(3000);
    check("wait/notify moved 10 items: " + consumed, consumed == 10 && !consumer.isAlive());

    // 8. A timed wait expires on its own.
    long w0 = System.currentTimeMillis();
    synchronized (lock) {
      lock.wait(60);
    }
    long wdt = System.currentTimeMillis() - w0;
    check("wait(60) expired after " + wdt + " ms", wdt >= 50 && wdt < 500);

    // 9. wait() outside synchronized.
    threw = false;
    try {
      lock.wait();
    } catch (IllegalMonitorStateException e) {
      threw = true;
    }
    check("wait() outside synchronized throws IllegalMonitorStateException", threw);

    // 10. Recursion depth survives a wait.
    Thread waker =
        new Thread(
            () -> {
              quietSleep(30);
              synchronized (lock) {
                lock.notifyAll();
              }
            },
            "waker");
    boolean stillOwner = false;
    synchronized (lock) {
      synchronized (lock) {
        waker.start();
        lock.wait(2000);
      }
      // Still inside the outer block: notify must not throw.
      try {
        lock.notify();
        stillOwner = true;
      } catch (IllegalMonitorStateException e) {
        stillOwner = false;
      }
    }
    waker.join();
    check("monitor depth restored after wait", stillOwner);

    // 11. Uncaught exceptions reach the default handler.
    final Throwable[] caught = new Throwable[1];
    Thread.setDefaultUncaughtExceptionHandler((t, e) -> caught[0] = e);
    Thread bad =
        new Thread(
            () -> {
              throw new IllegalStateException("boom");
            },
            "bad");
    bad.start();
    bad.join();
    Thread.setDefaultUncaughtExceptionHandler(null);
    check("default uncaught handler saw the exception", caught[0] instanceof IllegalStateException);
    check("a thread that threw is dead", !bad.isAlive());

    // 12. The interrupt flag.
    Thread.currentThread().interrupt();
    boolean first = Thread.interrupted();
    boolean second = Thread.interrupted();
    check("interrupted() reads then clears the flag", first && !second);

    // 13. Start/join churn.
    for (int i = 0; i < 40; i++) {
      Thread t = new Thread(this::inc);
      t.start();
      t.join();
    }
    check("40 start/join cycles: " + get(), get() == 10040);
    Thread.yield();
    check("yield() returns", true);
  }

  private void loopInc(int n) {
    for (int i = 0; i < n; i++) {
      inc();
    }
  }

  private void consume() {
    synchronized (lock) {
      while (consumed < 10) {
        while (item < 0) {
          try {
            lock.wait();
          } catch (InterruptedException e) {
            return;
          }
        }
        item = -1;
        consumed = consumed + 1;
        lock.notifyAll();
      }
    }
  }

  private static void quietSleep(long ms) {
    try {
      Thread.sleep(ms);
    } catch (InterruptedException e) {
      // Woken early: nothing to do.
    }
  }
}
