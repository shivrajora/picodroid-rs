// SPDX-License-Identifier: GPL-3.0-only
package executorstress;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.os.Runtime;
import picodroid.util.Log;

/**
 * Posts lambdas whose ONLY reference is the queued executor word, then forces collections before
 * the queue drains. Each lambda proves its capture survived; the last one prints the verdict and
 * finishes the app.
 */
public class StressActivity extends Activity {
  private static final String TAG = "ExecutorStress";
  private static final int TASKS = 6;

  static int ran = 0;
  static int good = 0;

  private void post(final int id) {
    // The lambda and its captured box live only in the main-queue word once
    // this method returns.
    final Integer capture = Integer.valueOf(10_000 + id);
    Executors.mainExecutor()
        .execute(
            () -> {
              ran = ran + 1;
              if (capture != null && capture.intValue() == 10_000 + id) {
                good = good + 1;
              } else {
                Log.i(TAG, "task " + id + " capture corrupt: " + capture);
              }
              if (id == TASKS - 1) {
                Log.i(TAG, "ran=" + ran + " good=" + good + " gc=" + Runtime.gcCount());
                if (ran == TASKS && good == TASKS) {
                  Log.i(TAG, "=== ALL PASSED ===");
                } else {
                  Log.i(TAG, "=== FAILED ===");
                }
                finish();
              }
            });
  }

  @Override
  public void onCreate() {
    for (int i = 0; i < TASKS; i++) {
      post(i);
    }
    // Churn hard enough to force several GCs while the runnables are still
    // queued — their lambdas must be treated as roots or they are swept.
    int sink = 0;
    for (int i = 0; i < 4000; i++) {
      String s = "garbage-" + i;
      int[] a = new int[16];
      a[0] = s.length();
      sink += a[0];
    }
    Log.i(TAG, "churned, sink=" + sink + " gc=" + Runtime.gcCount());
  }
}
