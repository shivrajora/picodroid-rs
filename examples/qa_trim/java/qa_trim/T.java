// SPDX-License-Identifier: GPL-3.0-only
package qa_trim;

import picodroid.util.Log;

/** Shared trace + check ledger for the memory-pressure QA app. */
final class T {
  static final String TAG = "QaTrim";
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

  static void report() {
    Log.i(TAG, "trace: " + snapshot());
    Log.i(TAG, "checks: " + passed + " passed, " + failed + " failed");
    Log.i(TAG, failed == 0 ? "=== ALL PASSED ===" : "=== FAILED: " + failed + " ===");
  }
}
