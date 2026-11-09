// SPDX-License-Identifier: GPL-3.0-only
package qa_life;

import picodroid.concurrent.Executors;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/** Shared trace + check ledger for the lifecycle QA app. */
final class T {
  static final String TAG = "QaLife";
  static final StringBuilder trace = new StringBuilder();
  static int passed = 0;
  static int failed = 0;

  private T() {}

  static void log(String event) {
    Log.i(TAG, "event " + event);
    trace.append(event).append(' ');
  }

  static void check(String name, boolean condition) {
    if (condition) {
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  static String snapshot() {
    return trace.toString();
  }

  /** Index of the n-th (1-based) occurrence of token in the trace, or -1. */
  static int at(String token, int n) {
    String s = snapshot();
    int from = 0;
    for (int i = 0; i < n; i++) {
      int idx = s.indexOf(token + " ", from);
      if (idx < 0) {
        return -1;
      }
      if (i == n - 1) {
        return idx;
      }
      from = idx + 1;
    }
    return -1;
  }

  static int count(String token) {
    String s = snapshot();
    int c = 0;
    int from = 0;
    while (true) {
      int idx = s.indexOf(token + " ", from);
      if (idx < 0) {
        return c;
      }
      c++;
      from = idx + 1;
    }
  }

  static boolean before(String a, int na, String b, int nb) {
    int ia = at(a, na);
    int ib = at(b, nb);
    return ia >= 0 && ib >= 0 && ia < ib;
  }

  // later()'s pending tasks, guarded by `due`. One timer thread serves them all: a thread per
  // call asked for a fresh 16 KB task stack every step of the service chain while the last one
  // still awaited idle's reclaim, and the touch kit's fragmented heap refused one (nightly
  // 2026-10-27..31, largest block 14 KB). mainScheduledExecutor is not on testbench_rp2040.
  private static final Runnable[] due = new Runnable[8];
  private static final long[] dueAt = new long[8];
  private static boolean timerAlive;
  private static boolean reported;

  /** Runs r on the main thread after ms. */
  static void later(int ms, Runnable r) {
    synchronized (due) {
      int free = 0;
      while (free < due.length && due[free] != null) {
        free++;
      }
      if (free == due.length) {
        check("later() has a free slot", false);
        return;
      }
      due[free] = r;
      dueAt[free] = SystemClock.elapsedRealtime() + ms;
      if (timerAlive) {
        due.notify();
        return;
      }
      timerAlive = true;
    }
    try {
      new Thread(() -> runTimer(), "qa-life-timer").start();
    } catch (OutOfMemoryError e) {
      synchronized (due) {
        timerAlive = false;
      }
      throw e;
    }
  }

  /**
   * Posts each task when due; leaves after a second with nothing pending, or at once after the
   * report (a timer still waiting at teardown is interrupted, which the runtime logs as an error).
   */
  private static void runTimer() {
    synchronized (due) {
      long idleSince = SystemClock.elapsedRealtime();
      while (true) {
        long now = SystemClock.elapsedRealtime();
        long wait = -1; // until the soonest pending task; none pending: the idle rule below
        for (int i = 0; i < due.length; i++) {
          if (due[i] == null) {
            continue;
          }
          if (dueAt[i] <= now) {
            Executors.mainExecutor().execute(due[i]);
            due[i] = null;
            idleSince = now;
          } else if (wait < 0 || dueAt[i] - now < wait) {
            wait = dueAt[i] - now;
          }
        }
        if (wait < 0) {
          wait = reported ? 0 : 1000 - (now - idleSince);
        }
        if (wait <= 0) {
          timerAlive = false;
          return;
        }
        try {
          due.wait(wait);
        } catch (InterruptedException e) {
          timerAlive = false;
          return;
        }
      }
    }
  }

  static void report() {
    synchronized (due) {
      reported = true;
      due.notify();
    }
    Log.i(TAG, "trace: " + snapshot());
    Log.i(TAG, "passed=" + passed + " failed=" + failed);
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " ===");
    }
  }
}
