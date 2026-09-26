// SPDX-License-Identifier: GPL-3.0-only
package mainhog;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * A main thread that never idles must still share the interpreter with a sibling.
 *
 * <p>The UI loop gives up the JVM run lock around every blocking wait and takes it back after. A
 * main queue that is never empty makes that wait return at once, so the loop gives and retakes the
 * lock without ever blocking. On a single-core kernel, giving a mutex does not hand it to an
 * equal-priority waiter: the waiter is only made ready, and the giver, still running, takes the
 * mutex straight back. A child thread blocked on the lock then starves for as long as the queue
 * stays full (the simulator with a window open: the claudeusage poll thread's 4 s connect timeout
 * surfaced 16 to 30 s late, docs/designs/claudeusage-gaps-roadmap-2026-09.md D1).
 *
 * <p>Here a Runnable re-posts itself for {@link #HOG_MS}, so the queue is never empty, while a
 * child thread counts 1 ms sleeps; it then counts them again through an equally long window with
 * the main thread idle, and the hogged count must reach a quarter of the idle one. Passes on the
 * device's SMP kernel by construction (a woken equal-priority task preempts the giver there). In
 * the simulator it pins the run lock's hand-off: the giver yields after every give, so a sibling
 * that is ready — woken from a sleep and not yet at the mutex, which a waiter-only hand-off would
 * miss — runs and takes the lock first. Measured on the host simulator, 2 s hog: 1890 hogged sleeps
 * against 1893 idle with the hand-off, 42 to 73 without it.
 */
public class Main extends Activity {
  static final String TAG = "MainHog";

  /** How long the main queue is kept non-empty. */
  static final int HOG_MS = 2000;

  /** Sanity floor on the hogged count, for a host where the idle count itself is tiny. */
  static final int MIN_CHILD_TICKS = 10;

  static volatile int childTicks = 0;
  static volatile boolean hogDone = false;
  static volatile int hogTicks = 0;
  static long hogUntil;
  static long idleUntil;
  static int hops = 0;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "start");
    new Thread(Main::child, "mainhog-child").start();
    hogUntil = SystemClock.elapsedRealtime() + HOG_MS;
    Executors.mainExecutor().execute(Main::hog);
  }

  /** Counts sleeps through the hog, then through an equally long idle window, then reports. */
  static void child() {
    while (true) {
      SystemClock.sleep(1);
      childTicks = childTicks + 1;
      if (hogDone && SystemClock.elapsedRealtime() >= idleUntil) {
        break;
      }
    }
    int hog = hogTicks;
    int idle = childTicks - hog;
    Log.i(
        TAG,
        "hog: "
            + hops
            + " hops in "
            + HOG_MS
            + " ms, child slept "
            + hog
            + " times during the hog, "
            + idle
            + " times in the idle window after it");
    if (hog >= MIN_CHILD_TICKS && hog * 4 >= idle) {
      Log.i(TAG, "PASS child ran during the hog");
    } else {
      Log.i(TAG, "FAIL child starved during the hog");
    }
  }

  static void hog() {
    hops = hops + 1;
    if (SystemClock.elapsedRealtime() < hogUntil) {
      Executors.mainExecutor().execute(Main::hog);
      return;
    }
    hogTicks = childTicks;
    idleUntil = SystemClock.elapsedRealtime() + HOG_MS;
    hogDone = true;
  }
}
