// SPDX-License-Identifier: GPL-3.0-only
package fragmentdemo;

import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.TimeUnit;
import picodroid.util.Log;

/** Shared trace + check ledger, the qa_life shape. */
final class T {
  static final String TAG = "FragmentDemo";
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

  /** The main-thread deadline table, the SDK's delayed-work shape. */
  private static final ScheduledExecutorService TIMER =
      Executors.newSingleThreadScheduledExecutor();

  static void later(int ms, Runnable r) {
    TIMER.schedule(r, ms, TimeUnit.MILLISECONDS);
  }

  static void report() {
    Log.i(TAG, "trace: " + snapshot());
    Log.i(TAG, "passed=" + passed + " failed=" + failed);
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " ===");
    }
  }
}
