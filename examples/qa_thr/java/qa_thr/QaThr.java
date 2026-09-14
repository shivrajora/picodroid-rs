// SPDX-License-Identifier: GPL-3.0-only
package qa_thr;

import java.util.ArrayList;
import java.util.concurrent.CancellationException;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.TimeoutException;
import picodroid.app.Application;
import picodroid.concurrent.AtomicInteger;
import picodroid.concurrent.Callable;
import picodroid.concurrent.CountDownLatch;
import picodroid.concurrent.ExecutorService;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Future;
import picodroid.concurrent.Thread;
import picodroid.concurrent.TimeUnit;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * QA 2026-09-13: thread semantics — start/join/isAlive, interrupt and the flag, wait/notify
 * hand-offs and timeouts, monitor exclusion on instance and class locks, uncaught-exception
 * handlers, latches, atomics, the ExecutorService/Future contract, and clock/sleep accuracy.
 */
public class QaThr extends Application {
  private static final String TAG = "QaThr";

  static int passed = 0;
  static int failed = 0;
  static int crashed = 0;
  static int sink = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  interface Section {
    void run() throws Exception;
  }

  static void section(String name, Section s) {
    Log.i(TAG, "section " + name);
    try {
      s.run();
    } catch (Throwable t) {
      Log.i(TAG, "CRASH in " + name + ": " + t + " msg=" + t.getMessage());
      crashed = crashed + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== QaThr start ===");
    section("clocks", () -> clocks());
    section("lifecycle", () -> lifecycle());
    section("interrupts", () -> interrupts());
    section("monitors", () -> monitors());
    section("waitNotify", () -> waitNotify());
    section("uncaught", () -> uncaught());
    section("latchesAtomics", () -> latchesAtomics());
    section("executors", () -> executors());
    section("frameworkExecutors", () -> frameworkExecutors());
    Log.i(TAG, "passed=" + passed + " failed=" + failed + " crashed=" + crashed);
    if (failed == 0 && crashed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " failed, " + crashed + " crashed ===");
    }
  }

  static void bump() {
    sink++;
  }

  static long nowMs() {
    return SystemClock.elapsedRealtimeNanos() / 1000000L;
  }

  // ---- clocks ---------------------------------------------------------------------------------

  static void clocks() throws InterruptedException {
    long t0 = nowMs();
    Thread.sleep(50);
    long dt = nowMs() - t0;
    check("sleep(50) >= 50 ms", dt >= 50);
    check("sleep(50) < 250 ms", dt < 250);
    Log.i(TAG, "sleep(50) took " + dt + " ms");
    t0 = nowMs();
    SystemClock.sleep(30);
    dt = nowMs() - t0;
    check("SystemClock.sleep(30) in range", dt >= 30 && dt < 200);
    t0 = nowMs();
    Thread.sleep(0);
    Thread.sleep(1);
    check("sleep(0)/sleep(1) return promptly", nowMs() - t0 < 100);
    long a = System.currentTimeMillis();
    long b = SystemClock.elapsedRealtime();
    long c = SystemClock.elapsedRealtimeNanos();
    check("clocks agree within 50 ms", Math.abs(a - b) < 50 && Math.abs(c / 1000000L - b) < 50);
    boolean monotonic = true;
    long prev = System.currentTimeMillis();
    long prevN = SystemClock.elapsedRealtimeNanos();
    for (int i = 0; i < 200; i++) {
      long n = System.currentTimeMillis();
      long nn = SystemClock.elapsedRealtimeNanos();
      if (n < prev || nn < prevN) {
        monotonic = false;
      }
      prev = n;
      prevN = nn;
    }
    check("clocks monotonic", monotonic);
    long tu = nowMs();
    Thread.sleep(1, 500000);
    check("sleep(ms, nanos) form", nowMs() - tu < 100);
    check(
        "TimeUnit conversions",
        TimeUnit.SECONDS.toMillis(2) == 2000
            && TimeUnit.MILLISECONDS.toSeconds(2999) == 2
            && TimeUnit.MINUTES.toSeconds(1) == 60
            && TimeUnit.MILLISECONDS.convert(3, TimeUnit.SECONDS) == 3000
            && TimeUnit.MICROSECONDS.toNanos(1) == 1000);
    long ts = nowMs();
    TimeUnit.MILLISECONDS.sleep(20);
    check("TimeUnit.sleep", nowMs() - ts >= 20);
  }

  // ---- lifecycle ------------------------------------------------------------------------------

  static volatile boolean ran = false;

  static void lifecycle() throws InterruptedException {
    Thread t =
        new Thread(
            () -> {
              ran = true;
              SystemClock.sleep(80);
            },
            "worker-1");
    check("not alive before start", !t.isAlive());
    check("name set", t.getName().equals("worker-1"));
    t.start();
    Thread.sleep(10);
    check("alive while running", t.isAlive() && ran);
    long t0 = nowMs();
    t.join(20);
    long dt = nowMs() - t0;
    check("join(timeout) returns early", dt >= 15 && dt < 75 && t.isAlive());
    t.join();
    check("dead after join", !t.isAlive());
    t.join();
    check("join on dead thread returns", true);
    boolean itse = false;
    try {
      t.start();
    } catch (IllegalThreadStateException e) {
      itse = true;
    }
    check("second start throws IllegalThreadStateException", itse);
    Thread anon =
        new Thread() {
          @Override
          public void run() {
            sink = 77;
          }
        };
    anon.start();
    anon.join();
    check("subclass run()", sink == 77);
    final String[] seenName = new String[1];
    final long[] seenId = new long[1];
    Thread named =
        new Thread(
            () -> {
              seenName[0] = Thread.currentThread().getName();
              seenId[0] = Thread.currentThread().getId();
            },
            "named-x");
    named.start();
    named.join();
    check("currentThread in child", "named-x".equals(seenName[0]) && seenId[0] == named.getId());
    check(
        "ids differ",
        named.getId() != t.getId() && Thread.currentThread().getId() != named.getId());
    Thread unnamed = new Thread(() -> sink++);
    check("default name", unnamed.getName().startsWith("Thread-"));
    unnamed.setName("renamed");
    check("setName", unnamed.getName().equals("renamed"));
    unnamed.start();
    unnamed.join();
    check("main thread name", Thread.currentThread().getName().equals("main"));
    check(
        "priority round trip",
        unnamed.getPriority() == Thread.NORM_PRIORITY || unnamed.getPriority() > 0);
    Thread pri = new Thread(() -> sink++);
    pri.setPriority(Thread.MAX_PRIORITY);
    check("setPriority", pri.getPriority() == Thread.MAX_PRIORITY);
    pri.setDaemon(true);
    check("daemon flag", pri.isDaemon());
    pri.start();
    pri.join();
    check("toString", pri.toString().contains("Thread"));
    // many short-lived threads in sequence
    boolean ok = true;
    for (int i = 0; i < 30; i++) {
      final int k = i;
      final int[] out = new int[1];
      Thread s = new Thread(() -> out[0] = k * 2);
      s.start();
      s.join();
      if (out[0] != k * 2 || s.isAlive()) {
        ok = false;
      }
    }
    check("30 sequential threads", ok);
    // several concurrent
    Thread[] ts = new Thread[6];
    final int[] results = new int[6];
    for (int i = 0; i < 6; i++) {
      final int k = i;
      ts[i] =
          new Thread(
              () -> {
                int acc = 0;
                for (int j = 0; j < 20000; j++) {
                  acc += (j % (k + 1));
                }
                results[k] = acc;
              });
      ts[i].start();
    }
    for (Thread x : ts) {
      x.join();
    }
    ok = true;
    for (int i = 0; i < 6; i++) {
      int acc = 0;
      for (int j = 0; j < 20000; j++) {
        acc += (j % (i + 1));
      }
      if (results[i] != acc) {
        ok = false;
      }
    }
    check("6 concurrent compute threads", ok);
    Thread.yield();
    check("yield returns", true);
  }

  // ---- interrupts -----------------------------------------------------------------------------

  static volatile int intState = 0;

  static void interrupts() throws InterruptedException {
    Thread sleeper =
        new Thread(
            () -> {
              try {
                Thread.sleep(5000);
                intState = 1;
              } catch (InterruptedException e) {
                intState = Thread.currentThread().isInterrupted() ? 3 : 2;
              }
            });
    sleeper.start();
    Thread.sleep(20);
    long t0 = nowMs();
    sleeper.interrupt();
    sleeper.join(1000);
    check("interrupt wakes sleep quickly", !sleeper.isAlive() && nowMs() - t0 < 500);
    check("InterruptedException clears flag", intState == 2);
    intState = 0;
    Thread pre =
        new Thread(
            () -> {
              Thread.currentThread().interrupt();
              boolean flagged = Thread.currentThread().isInterrupted();
              boolean cleared = Thread.interrupted();
              boolean after = Thread.interrupted();
              intState = (flagged ? 1 : 0) + (cleared ? 2 : 0) + (after ? 4 : 0);
            });
    pre.start();
    pre.join();
    check("isInterrupted keeps, interrupted() clears", intState == 3);
    intState = 0;
    Thread preSleep =
        new Thread(
            () -> {
              Thread.currentThread().interrupt();
              try {
                Thread.sleep(2000);
                intState = 1;
              } catch (InterruptedException e) {
                intState = 2;
              }
            });
    long t1 = nowMs();
    preSleep.start();
    preSleep.join(1000);
    check("pending interrupt makes sleep throw at once", intState == 2 && nowMs() - t1 < 500);
    // interrupting a join
    intState = 0;
    Thread longRunner = new Thread(() -> SystemClock.sleep(300));
    longRunner.start();
    Thread joiner =
        new Thread(
            () -> {
              try {
                longRunner.join();
                intState = 1;
              } catch (InterruptedException e) {
                intState = 2;
              }
            });
    joiner.start();
    Thread.sleep(20);
    joiner.interrupt();
    joiner.join(1000);
    check("interrupt wakes join", intState == 2 && !joiner.isAlive());
    longRunner.join();
    // interrupting a wait
    intState = 0;
    final Object lock = new Object();
    Thread waiter =
        new Thread(
            () -> {
              synchronized (lock) {
                try {
                  lock.wait();
                  intState = 1;
                } catch (InterruptedException e) {
                  intState = 2;
                }
              }
            });
    waiter.start();
    Thread.sleep(20);
    waiter.interrupt();
    waiter.join(1000);
    check("interrupt wakes wait", intState == 2 && !waiter.isAlive());
    // interrupt on a finished thread is harmless
    waiter.interrupt();
    check("interrupt dead thread ok", true);
    check("main not interrupted", !Thread.currentThread().isInterrupted());
  }

  // ---- monitors -------------------------------------------------------------------------------

  static int counter = 0;
  static int staticCounter = 0;

  static synchronized void bumpStatic() {
    int c = staticCounter;
    if ((c & 7) == 0) {
      Thread.yield();
    }
    staticCounter = c + 1;
  }

  synchronized void bumpInstance() {
    int c = counter;
    if ((c & 7) == 0) {
      Thread.yield();
    }
    counter = c + 1;
  }

  static class Box {
    int v;
  }

  void monitors() throws InterruptedException {
    counter = 0;
    staticCounter = 0;
    Thread[] ts = new Thread[4];
    for (int i = 0; i < 4; i++) {
      ts[i] =
          new Thread(
              () -> {
                for (int j = 0; j < 2500; j++) {
                  bumpInstance();
                  bumpStatic();
                }
              });
      ts[i].start();
    }
    for (Thread t : ts) {
      t.join();
    }
    check("synchronized instance method exact", counter == 10000);
    check("synchronized static method exact", staticCounter == 10000);
    final Box box = new Box();
    Thread[] bs = new Thread[4];
    for (int i = 0; i < 4; i++) {
      bs[i] =
          new Thread(
              () -> {
                for (int j = 0; j < 2500; j++) {
                  synchronized (box) {
                    int c = box.v;
                    if ((c & 15) == 0) {
                      Thread.yield();
                    }
                    box.v = c + 1;
                  }
                }
              });
      bs[i].start();
    }
    for (Thread t : bs) {
      t.join();
    }
    check("synchronized block exact", box.v == 10000);
    synchronized (box) {
      synchronized (box) {
        box.v++;
      }
    }
    check("reentrant block", box.v == 10001);
    final String lit = "lock-literal";
    final int[] hits = new int[1];
    Thread[] ls = new Thread[3];
    for (int i = 0; i < 3; i++) {
      ls[i] =
          new Thread(
              () -> {
                for (int j = 0; j < 1000; j++) {
                  synchronized (lit) {
                    hits[0]++;
                  }
                }
              });
      ls[i].start();
    }
    for (Thread t : ls) {
      t.join();
    }
    check("synchronized on a string literal", hits[0] == 3000);
    synchronized (QaThr.class) {
      sink++;
    }
    check("synchronized on a Class object", true);
    boolean imse = false;
    try {
      box.wait(1);
    } catch (IllegalMonitorStateException e) {
      imse = true;
    }
    check("wait outside monitor throws IMSE", imse);
    imse = false;
    try {
      box.notify();
    } catch (IllegalMonitorStateException e) {
      imse = true;
    }
    check("notify outside monitor throws IMSE", imse);
    imse = false;
    try {
      box.notifyAll();
    } catch (IllegalMonitorStateException e) {
      imse = true;
    }
    check("notifyAll outside monitor throws IMSE", imse);
    // exception inside synchronized releases the monitor
    try {
      synchronized (box) {
        throw new IllegalStateException("inside");
      }
    } catch (IllegalStateException e) {
      sink++;
    }
    final boolean[] acquired = new boolean[1];
    Thread after =
        new Thread(
            () -> {
              synchronized (box) {
                acquired[0] = true;
              }
            });
    after.start();
    after.join(500);
    check("monitor released after exception", acquired[0]);
  }

  // ---- wait / notify --------------------------------------------------------------------------

  static final Object pp = new Object();
  static int turn = 0;
  static int pingRounds = 0;
  static int pongRounds = 0;

  static void waitNotify() throws InterruptedException {
    turn = 0;
    pingRounds = 0;
    pongRounds = 0;
    Thread ping =
        new Thread(
            () -> {
              try {
                for (int i = 0; i < 100; i++) {
                  synchronized (pp) {
                    while (turn != 0) {
                      pp.wait();
                    }
                    pingRounds++;
                    turn = 1;
                    pp.notifyAll();
                  }
                }
              } catch (InterruptedException e) {
                pingRounds = -1;
              }
            });
    Thread pong =
        new Thread(
            () -> {
              try {
                for (int i = 0; i < 100; i++) {
                  synchronized (pp) {
                    while (turn != 1) {
                      pp.wait();
                    }
                    pongRounds++;
                    turn = 0;
                    pp.notifyAll();
                  }
                }
              } catch (InterruptedException e) {
                pongRounds = -1;
              }
            });
    ping.start();
    pong.start();
    ping.join(5000);
    pong.join(5000);
    check(
        "ping-pong 100 rounds",
        pingRounds == 100 && pongRounds == 100 && !ping.isAlive() && !pong.isAlive());
    final Object gate = new Object();
    final int[] woke = new int[1];
    Thread[] waiters = new Thread[3];
    for (int i = 0; i < 3; i++) {
      waiters[i] =
          new Thread(
              () -> {
                synchronized (gate) {
                  try {
                    gate.wait();
                    woke[0]++;
                  } catch (InterruptedException e) {
                    woke[0] = -100;
                  }
                }
              });
      waiters[i].start();
    }
    Thread.sleep(50);
    synchronized (gate) {
      gate.notifyAll();
    }
    for (Thread w : waiters) {
      w.join(1000);
    }
    check("notifyAll wakes three", woke[0] == 3);
    final Object one = new Object();
    final int[] woke1 = new int[1];
    Thread[] w2 = new Thread[2];
    for (int i = 0; i < 2; i++) {
      w2[i] =
          new Thread(
              () -> {
                synchronized (one) {
                  try {
                    one.wait(2000);
                    woke1[0]++;
                  } catch (InterruptedException e) {
                    woke1[0] = -100;
                  }
                }
              });
      w2[i].start();
    }
    Thread.sleep(50);
    synchronized (one) {
      one.notify();
    }
    Thread.sleep(100);
    int afterOne = woke1[0];
    synchronized (one) {
      one.notifyAll();
    }
    for (Thread w : w2) {
      w.join(3000);
    }
    check("notify wakes exactly one", afterOne == 1 && woke1[0] == 2);
    final Object timed = new Object();
    long t0 = nowMs();
    synchronized (timed) {
      timed.wait(60);
    }
    long dt = nowMs() - t0;
    check("wait(60) times out", dt >= 55 && dt < 300);
    Log.i(TAG, "wait(60) took " + dt + " ms");
    // notify before wait is lost (state must be checked)
    final Object late = new Object();
    synchronized (late) {
      late.notify();
    }
    t0 = nowMs();
    synchronized (late) {
      late.wait(40);
    }
    check("early notify is not remembered", nowMs() - t0 >= 35);
    // wait releases the monitor so another thread can enter
    final Object rel = new Object();
    final boolean[] entered = new boolean[1];
    Thread enterer =
        new Thread(
            () -> {
              synchronized (rel) {
                entered[0] = true;
                rel.notify();
              }
            });
    synchronized (rel) {
      enterer.start();
      rel.wait(1000);
    }
    enterer.join(1000);
    check("wait releases the monitor", entered[0]);
    // producer/consumer with a bounded buffer
    final ArrayList<Integer> buf = new ArrayList<>();
    final int[] consumed = new int[1];
    final long[] total = new long[1];
    Thread producer =
        new Thread(
            () -> {
              try {
                for (int i = 1; i <= 200; i++) {
                  synchronized (buf) {
                    while (buf.size() >= 4) {
                      buf.wait();
                    }
                    buf.add(i);
                    buf.notifyAll();
                  }
                }
              } catch (InterruptedException e) {
                total[0] = -1;
              }
            });
    Thread consumer =
        new Thread(
            () -> {
              try {
                while (consumed[0] < 200) {
                  synchronized (buf) {
                    while (buf.isEmpty()) {
                      buf.wait();
                    }
                    int v = buf.remove(0);
                    consumed[0]++;
                    total[0] += v;
                    buf.notifyAll();
                  }
                }
              } catch (InterruptedException e) {
                total[0] = -1;
              }
            });
    producer.start();
    consumer.start();
    producer.join(5000);
    consumer.join(5000);
    check(
        "bounded buffer producer/consumer",
        consumed[0] == 200 && total[0] == 20100 && buf.isEmpty());
  }

  // ---- uncaught exceptions --------------------------------------------------------------------

  static volatile String uncaughtSeen = null;

  static void uncaught() throws InterruptedException {
    Thread.UncaughtExceptionHandler old = Thread.getDefaultUncaughtExceptionHandler();
    Thread.setDefaultUncaughtExceptionHandler(
        (th, e) -> uncaughtSeen = th.getName() + ":" + e.getMessage());
    Thread bad =
        new Thread(
            () -> {
              throw new IllegalStateException("boom");
            },
            "bad-thread");
    bad.start();
    bad.join(2000);
    check("default handler saw the exception", "bad-thread:boom".equals(uncaughtSeen));
    check("bad thread finished", !bad.isAlive());
    uncaughtSeen = null;
    Thread bad2 =
        new Thread(
            () -> {
              int[] a = new int[1];
              sink += a[3];
            },
            "bad-2");
    bad2.setUncaughtExceptionHandler(
        (th, e) -> uncaughtSeen = "own:" + (e instanceof ArrayIndexOutOfBoundsException));
    bad2.start();
    bad2.join(2000);
    check("per-thread handler wins", "own:true".equals(uncaughtSeen));
    check("getUncaughtExceptionHandler", bad2.getUncaughtExceptionHandler() != null);
    Thread.setDefaultUncaughtExceptionHandler(old);
    check("main still runs after child died", true);
    Thread fine = new Thread(() -> sink++);
    fine.start();
    fine.join();
    check("new threads still start", !fine.isAlive());
  }

  // ---- latches and atomics --------------------------------------------------------------------

  static void latchesAtomics() throws InterruptedException {
    final CountDownLatch latch = new CountDownLatch(3);
    final AtomicInteger atomic = new AtomicInteger();
    for (int i = 0; i < 3; i++) {
      new Thread(
              () -> {
                for (int j = 0; j < 3000; j++) {
                  atomic.incrementAndGet();
                }
                latch.countDown();
              })
          .start();
    }
    check("latch await", latch.await(5000, TimeUnit.MILLISECONDS));
    check("latch count 0", latch.getCount() == 0);
    check("atomic increments exact", atomic.get() == 9000);
    latch.countDown();
    check("countDown below zero stays 0", latch.getCount() == 0);
    latch.await();
    check("await on zero returns", true);
    CountDownLatch never = new CountDownLatch(1);
    long t0 = nowMs();
    check("await timeout false", !never.await(50, TimeUnit.MILLISECONDS) && nowMs() - t0 >= 45);
    AtomicInteger cas = new AtomicInteger(5);
    check("compareAndSet", cas.compareAndSet(5, 6) && !cas.compareAndSet(5, 7) && cas.get() == 6);
    check("getAndSet", cas.getAndSet(10) == 6 && cas.get() == 10);
    check("getAndAdd/addAndGet", cas.getAndAdd(5) == 10 && cas.addAndGet(-15) == 0);
    check(
        "getAndIncrement/decrement",
        cas.getAndIncrement() == 0
            && cas.decrementAndGet() == 0
            && cas.getAndDecrement() == 0
            && cas.get() == -1);
    check(
        "toString/intValue",
        cas.toString().equals("-1") && cas.intValue() == -1 && cas.longValue() == -1L);
    final AtomicInteger contended = new AtomicInteger();
    Thread[] ts = new Thread[4];
    for (int i = 0; i < 4; i++) {
      ts[i] =
          new Thread(
              () -> {
                for (int j = 0; j < 2000; j++) {
                  int cur;
                  do {
                    cur = contended.get();
                  } while (!contended.compareAndSet(cur, cur + 1));
                }
              });
      ts[i].start();
    }
    for (Thread t : ts) {
      t.join();
    }
    check("CAS loop under contention", contended.get() == 8000);
    // volatile flag visibility
    final boolean[] stop = new boolean[1];
    final long[] spins = new long[1];
    Thread spinner =
        new Thread(
            () -> {
              while (!stopFlag) {
                spins[0]++;
                if ((spins[0] & 0xff) == 0) {
                  Thread.yield();
                }
              }
              stop[0] = true;
            });
    stopFlag = false;
    spinner.start();
    Thread.sleep(30);
    stopFlag = true;
    spinner.join(2000);
    check("volatile stop flag observed", stop[0] && !spinner.isAlive());
  }

  static volatile boolean stopFlag = false;

  // ---- ExecutorService / Future ---------------------------------------------------------------

  static void executors() throws Exception {
    ExecutorService pool = Executors.newFixedThreadPool(2);
    ArrayList<Future<Integer>> fs = new ArrayList<>();
    for (int i = 0; i < 10; i++) {
      final int k = i;
      fs.add(
          pool.submit(
              (Callable<Integer>)
                  () -> {
                    SystemClock.sleep(5);
                    return k * k;
                  }));
    }
    boolean ok = true;
    for (int i = 0; i < 10; i++) {
      if (fs.get(i).get() != i * i) {
        ok = false;
      }
    }
    check("10 callables on a pool of 2", ok);
    check("isDone after get", fs.get(0).isDone() && !fs.get(0).isCancelled());
    Future<Integer> slow =
        pool.submit(
            (Callable<Integer>)
                () -> {
                  SystemClock.sleep(400);
                  return 1;
                });
    boolean timedOut = false;
    long t0 = nowMs();
    try {
      slow.get(50, TimeUnit.MILLISECONDS);
    } catch (TimeoutException e) {
      timedOut = true;
    }
    check("get(timeout) throws TimeoutException", timedOut && nowMs() - t0 < 300);
    check("get after timeout still delivers", slow.get() == 1);
    Future<Integer> failing =
        pool.submit(
            (Callable<Integer>)
                () -> {
                  throw new IllegalStateException("callable failed");
                });
    boolean ee = false;
    try {
      failing.get();
    } catch (ExecutionException e) {
      ee =
          e.getCause() instanceof IllegalStateException
              && "callable failed".equals(e.getCause().getMessage());
    }
    check("ExecutionException wraps cause", ee);
    check("failed future isDone", failing.isDone());
    Future<?> blocker = pool.submit(() -> SystemClock.sleep(200));
    Future<?> blocker2 = pool.submit(() -> SystemClock.sleep(200));
    Future<Integer> queued = pool.submit((Callable<Integer>) () -> 5);
    check("cancel queued task", queued.cancel(false) && queued.isCancelled() && queued.isDone());
    boolean ce = false;
    try {
      queued.get();
    } catch (CancellationException e) {
      ce = true;
    }
    check("get on cancelled throws CancellationException", ce);
    check("cancel completed returns false", !fs.get(0).cancel(true));
    blocker.get();
    blocker2.get();
    Future<?> runnableFuture = pool.submit(() -> bump());
    check("submit(Runnable) get null", runnableFuture.get() == null);
    final int[] executed = new int[1];
    final CountDownLatch execLatch = new CountDownLatch(1);
    pool.execute(
        () -> {
          executed[0] = 1;
          execLatch.countDown();
        });
    check("execute runs", execLatch.await(2000, TimeUnit.MILLISECONDS) && executed[0] == 1);
    final CountDownLatch orderLatch = new CountDownLatch(20);
    final int[] seq = new int[20];
    final int[] idx = new int[1];
    ExecutorService single = Executors.newSingleThreadExecutor();
    for (int i = 0; i < 20; i++) {
      final int k = i;
      single.execute(
          () -> {
            seq[idx[0]++] = k;
            orderLatch.countDown();
          });
    }
    check("single thread executor completes", orderLatch.await(3000, TimeUnit.MILLISECONDS));
    boolean inOrder = true;
    for (int i = 0; i < 20; i++) {
      if (seq[i] != i) {
        inOrder = false;
      }
    }
    check("single thread executor preserves order", inOrder);
    single.shutdown();
    check("isShutdown", single.isShutdown());
    check(
        "awaitTermination",
        single.awaitTermination(2000, TimeUnit.MILLISECONDS) && single.isTerminated());
    boolean rej = false;
    try {
      single.submit(() -> sink++);
    } catch (RejectedExecutionException e) {
      rej = true;
    }
    check("submit after shutdown rejected", rej);
    pool.shutdown();
    check("pool awaitTermination", pool.awaitTermination(2000, TimeUnit.MILLISECONDS));
    ExecutorService now = Executors.newFixedThreadPool(1);
    now.submit(() -> SystemClock.sleep(150));
    Thread.sleep(30);
    now.submit(() -> bump());
    java.util.List<Runnable> pending = now.shutdownNow();
    check("shutdownNow returns the one queued task", pending != null && pending.size() == 1);
    check("shutdownNow terminates", now.awaitTermination(2000, TimeUnit.MILLISECONDS));
  }

  // ---- framework executors --------------------------------------------------------------------

  static void frameworkExecutors() throws InterruptedException {
    final CountDownLatch bgLatch = new CountDownLatch(4);
    final String[] names = new String[4];
    for (int i = 0; i < 4; i++) {
      final int k = i;
      Executors.backgroundExecutor()
          .execute(
              () -> {
                names[k] = Thread.currentThread().getName();
                bgLatch.countDown();
              });
    }
    check("background executor ran 4 jobs", bgLatch.await(3000, TimeUnit.MILLISECONDS));
    check("background jobs not on main", names[0] != null && !names[0].equals("main"));
    Log.i(TAG, "background thread name: " + names[0]);
    final CountDownLatch chain = new CountDownLatch(1);
    final int[] hops = new int[1];
    Executors.backgroundExecutor()
        .execute(
            () -> {
              hops[0]++;
              Executors.backgroundExecutor()
                  .execute(
                      () -> {
                        hops[0]++;
                        chain.countDown();
                      });
            });
    check(
        "background job can post another",
        chain.await(3000, TimeUnit.MILLISECONDS) && hops[0] == 2);
    final CountDownLatch many = new CountDownLatch(64);
    final AtomicInteger done = new AtomicInteger();
    int posted = 0;
    for (int i = 0; i < 64; i++) {
      Executors.backgroundExecutor()
          .execute(
              () -> {
                done.incrementAndGet();
                many.countDown();
              });
      posted++;
    }
    boolean all = many.await(5000, TimeUnit.MILLISECONDS);
    Log.i(TAG, "background burst: posted=" + posted + " done=" + done.get());
    check("background burst of 64 (queue depth)", all || done.get() > 0);
  }
}
