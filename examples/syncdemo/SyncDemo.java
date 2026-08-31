// SPDX-License-Identifier: GPL-3.0-only
package syncdemo;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Demonstrates Java synchronized blocks (monitorenter / monitorexit).
 *
 * <p>Single-threaded: verifies the bytecodes are handled and reentrant locking works. The monitors
 * are real kernel mutexes on device and in the simulator alike; {@code examples/threadparity}
 * exercises them under contention (including synchronized methods, wait/notify).
 */
public class SyncDemo extends Application {
  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    Log.i("SyncDemo", "Starting synchronized demo");

    int counter = 0;
    Object lock = new Object();

    // Basic synchronized block
    synchronized (lock) {
      counter = counter + 1;
    }
    Log.i("SyncDemo", "After first sync block: counter = " + counter);

    // Reentrant: nested synchronized on the same lock
    synchronized (lock) {
      synchronized (lock) {
        counter = counter + 1;
      }
    }
    Log.i("SyncDemo", "After reentrant sync: counter = " + counter);

    // Multiple iterations
    for (int i = 0; i < 5; i++) {
      synchronized (lock) {
        counter = counter + 1;
      }
    }
    Log.i("SyncDemo", "After loop: counter = " + counter);

    Log.i("SyncDemo", "Done. Final counter = " + counter);
  }
}
