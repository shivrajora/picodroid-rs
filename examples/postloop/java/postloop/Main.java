// SPDX-License-Identifier: GPL-3.0-only
package postloop;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.Context;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Minimal reproduction of the qa_life stall: one fresh child Thread per hop, which sleeps 80 ms and
 * posts the next hop to the main executor. Every tenth hop also commits a SharedPreferences write,
 * so a hop lands next to a flash erase/program window — the qa_life stalls follow a slow,
 * flash-bound pending-op drain.
 *
 * <p>One log line per hop, carrying the per-stage millisecond costs, so a slow or lost hop names
 * the stage that ate the time: start (Thread.start), up (child's first instruction), slept (the 80
 * ms sleep), post (the execute call), deliver (post to the UI loop running it).
 */
public class Main extends Activity {
  static final String TAG = "PostLoop";
  static final int HOPS = 300;
  static int n = 0;
  static Main self;

  static long tSpawn = 0;
  static long tStarted = 0;
  static long tUp = 0;
  static long tAwake = 0;
  static long tPosted = 0;

  @Override
  public void onCreate() {
    self = this;
    Log.i(TAG, "start");
    hop();
  }

  static long ms(long a, long b) {
    return (b - a) / 1000000L;
  }

  static void hop() {
    n = n + 1;
    if (n > HOPS) {
      Log.i(TAG, "=== ALL PASSED ===");
      return;
    }
    if (n % 10 == 0) {
      self.getSharedPreferences("postloop", Context.MODE_PRIVATE).edit().putInt("n", n).commit();
    }
    tSpawn = SystemClock.elapsedRealtimeNanos();
    new Thread(
            () -> {
              tUp = SystemClock.elapsedRealtimeNanos();
              SystemClock.sleep(80);
              tAwake = SystemClock.elapsedRealtimeNanos();
              tPosted = tAwake;
              Executors.mainExecutor()
                  .execute(
                      () -> {
                        long tRan = SystemClock.elapsedRealtimeNanos();
                        Log.i(
                            TAG,
                            "hop "
                                + n
                                + " start="
                                + ms(tSpawn, tStarted)
                                + " up="
                                + ms(tStarted, tUp)
                                + " slept="
                                + ms(tUp, tAwake)
                                + " post="
                                + ms(tAwake, tPosted)
                                + " deliver="
                                + ms(tPosted, tRan));
                        hop();
                      });
            },
            "postloop-timer")
        .start();
    tStarted = SystemClock.elapsedRealtimeNanos();
  }
}
