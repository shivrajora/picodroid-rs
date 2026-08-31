// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

import picodroid.util.Log;

/**
 * A thread of execution, mirroring {@code java.lang.Thread}.
 *
 * <p>{@link #start()} runs {@link #run()} on a fresh RTOS task sharing the JVM heap. Either pass a
 * {@link Runnable} or subclass and override {@code run()}. {@link #sleep(long)}, {@link #join()},
 * {@link #interrupt()}, {@link #currentThread()} and {@code Object.wait()/notify()} behave as on
 * Android.
 *
 * <p>Divergence: {@link #setPriority(int)} is advisory. Every task that interprets Java runs at one
 * RTOS priority, because the shared heap relies on "a running JVM task keeps the core until it
 * blocks"; the value is stored and reported but never applied. {@link #setDaemon(boolean)} is
 * likewise recorded only — the process ends when the app does, not when the last non-daemon thread
 * exits.
 */
public class Thread implements Runnable {
  public static final int MIN_PRIORITY = 1;
  public static final int NORM_PRIORITY = 5;
  public static final int MAX_PRIORITY = 10;

  /** Handler for an exception that escaped a thread's {@link #run()}. */
  public interface UncaughtExceptionHandler {
    void uncaughtException(Thread t, Throwable e);
  }

  private static long nextId = 1;
  private static UncaughtExceptionHandler defaultHandler;

  private final Runnable target;
  private final long id;
  private String name;
  private int priority = NORM_PRIORITY;
  private boolean daemon;
  private boolean started;
  private UncaughtExceptionHandler handler;

  public Thread() {
    this(null, null);
  }

  public Thread(Runnable target) {
    this(target, null);
  }

  public Thread(String name) {
    this(null, name);
  }

  public Thread(Runnable target, String name) {
    this.target = target;
    this.id = nextId;
    nextId = nextId + 1;
    this.name = name != null ? name : "Thread-" + (id - 1);
  }

  /** Runs the target, if any. Subclasses override this instead of passing a target. */
  @Override
  public void run() {
    if (target != null) {
      target.run();
    }
  }

  /**
   * Starts this thread on its own task.
   *
   * @throws IllegalThreadStateException if it was already started
   */
  public void start() {
    if (started) {
      throw new IllegalThreadStateException();
    }
    started = true;
    if (!start0()) {
      started = false;
      throw new OutOfMemoryError("unable to create native thread: " + name);
    }
  }

  private native boolean start0();

  /** The {@code Thread} of the calling task; the UI task and executor workers get one lazily. */
  public static Thread currentThread() {
    Thread t = current0();
    if (t == null) {
      t = new Thread(currentKind0() == 0 ? "main" : null);
      t.started = true;
      t.adopt0();
    }
    return t;
  }

  private static native Thread current0();

  private static native int currentKind0();

  private native void adopt0();

  /**
   * Sleeps for {@code millis}. Unlike {@code SystemClock.sleep}, an {@link #interrupt()} ends it
   * early with an {@link InterruptedException}.
   */
  public static void sleep(long millis) throws InterruptedException {
    if (millis < 0) {
      throw new IllegalArgumentException("timeout value is negative");
    }
    sleep0(millis);
  }

  public static void sleep(long millis, int nanos) throws InterruptedException {
    if (nanos < 0 || nanos > 999999) {
      throw new IllegalArgumentException("nanosecond timeout value out of range");
    }
    sleep(nanos > 0 ? millis + 1 : millis);
  }

  private static native void sleep0(long millis) throws InterruptedException;

  /** Waits for this thread to finish. */
  public final void join() throws InterruptedException {
    join(0L);
  }

  /** Waits at most {@code millis} for this thread to finish; {@code 0} waits forever. */
  public final void join(long millis) throws InterruptedException {
    if (millis < 0) {
      throw new IllegalArgumentException("timeout value is negative");
    }
    join0(millis);
  }

  private native void join0(long millis) throws InterruptedException;

  /**
   * Interrupts this thread: a blocked {@link #sleep(long)}, {@link #join()} or {@code
   * Object.wait()} throws {@link InterruptedException}; otherwise the flag stays set until read.
   */
  public native void interrupt();

  public native boolean isInterrupted();

  /** The calling thread's interrupt flag, cleared. */
  public static native boolean interrupted();

  public final native boolean isAlive();

  /** A hint to the scheduler: let another ready task of the same priority run. */
  public static void yield() {
    yield0();
  }

  private static native void yield0();

  public final String getName() {
    return name;
  }

  public final void setName(String name) {
    if (name == null) {
      throw new NullPointerException("name cannot be null");
    }
    this.name = name;
  }

  public final boolean isDaemon() {
    return daemon;
  }

  public final void setDaemon(boolean on) {
    if (started) {
      throw new IllegalThreadStateException();
    }
    daemon = on;
  }

  public long getId() {
    return id;
  }

  public final int getPriority() {
    return priority;
  }

  /** Advisory — see the class comment. */
  public final void setPriority(int priority) {
    if (priority < MIN_PRIORITY || priority > MAX_PRIORITY) {
      throw new IllegalArgumentException("Priority out of range");
    }
    this.priority = priority;
  }

  public UncaughtExceptionHandler getUncaughtExceptionHandler() {
    return handler != null ? handler : defaultHandler;
  }

  public void setUncaughtExceptionHandler(UncaughtExceptionHandler eh) {
    handler = eh;
  }

  public static UncaughtExceptionHandler getDefaultUncaughtExceptionHandler() {
    return defaultHandler;
  }

  public static void setDefaultUncaughtExceptionHandler(UncaughtExceptionHandler eh) {
    defaultHandler = eh;
  }

  @Override
  public String toString() {
    return "Thread[" + name + "," + priority + "]";
  }

  /**
   * The whole life of a started thread, entered from the native side once the task exists: run,
   * route anything that escapes to the uncaught-exception handler, and always tell the registry we
   * are done so {@link #join()} returns and {@link #isAlive()} turns false.
   */
  static void runWrapper(Thread t) {
    try {
      t.run();
    } catch (Throwable e) {
      UncaughtExceptionHandler h = t.getUncaughtExceptionHandler();
      if (h != null) {
        try {
          h.uncaughtException(t, e);
        } catch (Throwable ignored) {
          Log.e("Thread", "uncaught-exception handler threw on \"" + t.name + "\"");
        }
      } else {
        Log.e("Thread", "Exception in thread \"" + t.name + "\" " + e);
      }
    } finally {
      t.exit0();
    }
  }

  private native void exit0();
}
