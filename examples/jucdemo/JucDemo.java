// SPDX-License-Identifier: GPL-3.0-only
package jucdemo;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CancellationException;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.TimeoutException;
import picodroid.app.Application;
import picodroid.concurrent.AtomicBoolean;
import picodroid.concurrent.AtomicInteger;
import picodroid.concurrent.AtomicLong;
import picodroid.concurrent.AtomicReference;
import picodroid.concurrent.CountDownLatch;
import picodroid.concurrent.ExecutorService;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Future;
import picodroid.concurrent.Thread;
import picodroid.concurrent.TimeUnit;
import picodroid.util.Log;

/**
 * End-to-end checks of the {@code java.util.concurrent} core set in {@code picodroid.concurrent}:
 * fixed and single-thread pools, {@code submit}/{@code Future.get} (plain, timed, cancelled,
 * failed), shutdown/awaitTermination/rejection, the atomics under contention, and CountDownLatch.
 * Logs one PASS/FAIL line per check and {@code === PASSED ===} when all hold.
 */
public class JucDemo extends Application {
  private static final String TAG = "JucDemo";
  private static int fails = 0;

  private static void check(String what, boolean ok) {
    Log.i(TAG, (ok ? "PASS: " : "FAIL: ") + what);
    if (!ok) {
      fails = fails + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== JucDemo start ===");
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

  private void runChecks() throws Exception {
    // 1. A fixed pool computes eight squares; the futures deliver them.
    ExecutorService pool = Executors.newFixedThreadPool(2);
    List<Future<Integer>> futures = new ArrayList<Future<Integer>>();
    for (int i = 0; i < 8; i++) {
      final int n = i;
      futures.add(pool.submit(() -> n * n));
    }
    int sum = 0;
    for (int i = 0; i < futures.size(); i++) {
      sum = sum + futures.get(i).get().intValue();
    }
    check("8 submitted Callables summed to 140: " + sum, sum == 140);
    check("a delivered future isDone", futures.get(0).isDone());

    // 2. submit(Runnable) completes with null. (A block lambda: an expression
    // lambda would resolve to the Callable overload, as on the JDK.)
    final int[] ran = new int[1];
    Future<?> fr =
        pool.submit(
            () -> {
              ran[0] = 1;
            });
    check("submit(Runnable) returns null and ran", fr.get() == null && ran[0] == 1);

    // 3. A throwing Callable surfaces as ExecutionException with the cause.
    Future<Object> boom =
        pool.submit(
            () -> {
              throw new IllegalStateException("boom");
            });
    boolean wrapped = false;
    try {
      boom.get();
    } catch (ExecutionException e) {
      wrapped = e.getCause() instanceof IllegalStateException;
    }
    check("ExecutionException carries the cause", wrapped);

    // 4. Timed get times out, then delivers.
    Future<Integer> slow =
        pool.submit(
            () -> {
              Thread.sleep(300);
              return 7;
            });
    boolean timedOut = false;
    try {
      slow.get(30, TimeUnit.MILLISECONDS);
    } catch (TimeoutException e) {
      timedOut = true;
    }
    check("get(30 ms) threw TimeoutException", timedOut);
    check("get() then delivered 7", slow.get().intValue() == 7);

    // 5. Cancel a queued task on a single worker.
    ExecutorService single = Executors.newSingleThreadExecutor();
    Future<Integer> blocker =
        single.submit(
            () -> {
              Thread.sleep(200);
              return 1;
            });
    Future<Integer> queued = single.submit(() -> 2);
    boolean cancelled = queued.cancel(false);
    boolean cancelledEx = false;
    try {
      queued.get();
    } catch (CancellationException e) {
      cancelledEx = true;
    }
    check("cancel() of a queued task", cancelled && queued.isCancelled() && cancelledEx);
    check("the running task still finished", blocker.get().intValue() == 1);

    // 6. Single worker runs tasks strictly in order.
    final StringBuilder order = new StringBuilder();
    for (int i = 0; i < 5; i++) {
      final int n = i;
      single.execute(() -> order.append(n));
    }
    single.shutdown();
    boolean terminated = single.awaitTermination(2, TimeUnit.SECONDS);
    check("single-thread executor kept FIFO order: " + order, "01234".equals(order.toString()));
    check("awaitTermination after shutdown", terminated && single.isTerminated());
    boolean rejected = false;
    try {
      single.execute(() -> {});
    } catch (RejectedExecutionException e) {
      rejected = true;
    }
    check("execute after shutdown is rejected", rejected && single.isShutdown());

    // 7. Atomics under contention.
    final AtomicInteger counter = new AtomicInteger();
    Thread[] adders = new Thread[3];
    for (int i = 0; i < adders.length; i++) {
      adders[i] =
          new Thread(
              () -> {
                for (int k = 0; k < 2000; k++) {
                  counter.incrementAndGet();
                }
              });
      adders[i].start();
    }
    for (int i = 0; i < adders.length; i++) {
      adders[i].join();
    }
    check("AtomicInteger: 3 x 2000 incrementAndGet = " + counter.get(), counter.get() == 6000);
    check("compareAndSet succeeds on match", counter.compareAndSet(6000, 1) && counter.get() == 1);
    check("compareAndSet fails on mismatch", !counter.compareAndSet(6000, 2) && counter.get() == 1);
    check("getAndAdd returns the old value", counter.getAndAdd(5) == 1 && counter.get() == 6);

    AtomicLong along = new AtomicLong(1L << 40);
    check("AtomicLong addAndGet", along.addAndGet(1) == (1L << 40) + 1);
    AtomicBoolean flag = new AtomicBoolean();
    check("AtomicBoolean compareAndSet", flag.compareAndSet(false, true) && flag.get());
    AtomicReference<String> ref = new AtomicReference<String>("a");
    check("AtomicReference getAndSet", "a".equals(ref.getAndSet("b")) && "b".equals(ref.get()));

    // 8. CountDownLatch fan-in and timed await.
    final CountDownLatch latch = new CountDownLatch(3);
    for (int i = 0; i < 3; i++) {
      new Thread(
              () -> {
                try {
                  Thread.sleep(20);
                } catch (InterruptedException e) {
                  // fall through
                }
                latch.countDown();
              })
          .start();
    }
    latch.await();
    check("CountDownLatch(3) opened", latch.getCount() == 0);
    check(
        "timed await on a closed latch returns false",
        !new CountDownLatch(1).await(40, TimeUnit.MILLISECONDS));

    // 9. Pool shutdownNow returns the tasks that never started. The worker
    // only runs once this task blocks (no time slicing), so wait for the
    // first task to be in flight before queueing the two that will drain.
    ExecutorService busy = Executors.newFixedThreadPool(1);
    final CountDownLatch started = new CountDownLatch(1);
    busy.execute(
        () -> {
          started.countDown();
          try {
            Thread.sleep(150);
          } catch (InterruptedException e) {
            // fall through
          }
        });
    started.await();
    busy.execute(() -> {});
    busy.execute(() -> {});
    List<Runnable> drained = busy.shutdownNow();
    check("shutdownNow drained the queued tasks: " + drained.size(), drained.size() == 2);
    check("pool terminates after shutdownNow", busy.awaitTermination(2, TimeUnit.SECONDS));

    pool.shutdown();
    check("fixed pool terminates", pool.awaitTermination(2, TimeUnit.SECONDS));
  }
}
