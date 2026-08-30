// SPDX-License-Identifier: GPL-3.0-only
package threadstress;

import java.util.ArrayList;
import picodroid.app.Application;
import picodroid.concurrent.Thread;
import picodroid.os.Runtime;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Concurrent-allocation stress for the compound-heap atomic sections (bugbash B0,
 * docs/followups-2026-08.md §1): three child threads plus the main task churn the shared heap —
 * ArrayList grow/shrink, string concat, array allocation — for a fixed window. Run under {@code
 * --mem-diag} with offensive checks armed; a span/overlap integrity trap or a corrupted digest is a
 * FAIL, a clean run logs {@code === PASSED ===}.
 */
public class ThreadStress extends Application {
  private static final String TAG = "ThreadStress";
  private static final int WINDOW_MS = 45000;
  static final int WORKERS = 3;

  static volatile int done = 0;
  static volatile int errors = 0;

  static class Churn implements Runnable {
    private final int id;

    Churn(int id) {
      this.id = id;
    }

    @Override
    public void run() {
      long end = SystemClock.elapsedRealtimeNanos() + WINDOW_MS * 1_000_000L;
      int rounds = 0;
      while (SystemClock.elapsedRealtimeNanos() < end) {
        ArrayList<String> l = new ArrayList<String>();
        for (int i = 0; i < 20; i++) {
          l.add("w" + id + "-" + i);
        }
        for (int i = 0; i < 10; i++) {
          l.remove(l.size() - 1);
        }
        int[] a = new int[64 + (rounds % 64)];
        a[a.length - 1] = id;
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < 8; i++) {
          sb.append(l.get(i)).append(',');
        }
        String s = sb.toString();
        if (s.length() < 8 || a[a.length - 1] != id || !l.get(0).equals("w" + id + "-0")) {
          errors++;
          Log.i(TAG, "worker " + id + " CORRUPT at round " + rounds);
        }
        rounds++;
        if (rounds % 500 == 0) {
          SystemClock.sleep(1); // let the others run
        }
      }
      Log.i(TAG, "worker " + id + " done, rounds=" + rounds);
      done = done + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== ThreadStress start (window " + WINDOW_MS + " ms) ===");
    for (int i = 0; i < WORKERS; i++) {
      new Thread(new Churn(i)).start();
    }
    // Main task churns too, then waits for the workers.
    new Churn(9).run();
    while (done < WORKERS) {
      SystemClock.sleep(50);
    }
    Log.i(TAG, "gc=" + Runtime.gcCount() + " freed=" + Runtime.gcFreed());
    if (errors == 0) {
      Log.i(TAG, "=== PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + errors + " ===");
    }
  }
}
