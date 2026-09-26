// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

public final class Executors {
  private Executors() {}

  /** The UI thread's executor: {@code execute} posts to the main loop ("runOnUiThread"). */
  public static native Executor mainExecutor();

  /** The framework's shared background pool. */
  public static native Executor backgroundExecutor();

  /**
   * A pool of {@code nThreads} worker {@link Thread}s over an unbounded FIFO queue, mirroring
   * {@code java.util.concurrent.Executors#newFixedThreadPool}. Each worker costs a task stack (16
   * KiB on the RP family); prefer {@link #backgroundExecutor()} for occasional work.
   */
  public static ExecutorService newFixedThreadPool(int nThreads) {
    if (nThreads <= 0) {
      throw new IllegalArgumentException("nThreads <= 0");
    }
    return new ThreadPoolExecutor(nThreads);
  }

  /** A single worker thread over an unbounded FIFO queue — tasks run strictly in order. */
  public static ExecutorService newSingleThreadExecutor() {
    return new ThreadPoolExecutor(1);
  }

  /**
   * A {@link ScheduledExecutorService} for delayed and periodic tasks, mirroring {@code
   * java.util.concurrent.Executors#newSingleThreadScheduledExecutor}. Its single thread is the
   * <em>main</em> thread: the runtime's frame tick posts each task when it is due, so it costs no
   * task stack and may touch widgets, and a task that blocks stalls the UI. Call {@code
   * shutdownNow()} when the owner goes away (an Activity's {@code onDestroy}), or the tasks keep
   * running against it.
   */
  public static ScheduledExecutorService newSingleThreadScheduledExecutor() {
    return new MainScheduledExecutor();
  }

  /**
   * Bridge invoked by the Rust-side drain of the main queue / background pool. Called directly via
   * {@code Jvm::invoke_static_with_args} with the queued Runnable as the single argument; routes
   * through {@code invokeinterface} bytecode so lambda proxies (which store their target method in
   * Rust-side metadata, not in a real vtable entry under {@code java/lang/Runnable}) resolve
   * correctly.
   */
  static void dispatchRunnable(Runnable r) {
    if (r != null) {
      r.run();
    }
  }
}
