// SPDX-License-Identifier: GPL-3.0-only
package reclaimdemo;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * Conformance app for reclaimed Activities: run it with "don't keep activities" on ({@code
 * PICODROID_DONT_KEEP_ACTIVITIES=1}) and every covered Activity is destroyed with its state saved,
 * then re-created from that Bundle when it is uncovered. Runs without input and ends in {@code ===
 * PASSED ===} or a {@code FAIL} line. With the switch off nothing is reclaimed and the trace check
 * fails, by design.
 */
public class ReclaimDemoApp extends Application {
  static final String TAG = "ReclaimDemo";
  static final StringBuilder trace = new StringBuilder();
  static int failures;

  static void check(String what, boolean ok) {
    if (!ok) {
      failures++;
      Log.e(TAG, "FAIL " + what);
    }
  }

  @Override
  public void onCreate() {
    startActivity(new Intent(HomeActivity.class));
  }
}
