package syncdemo;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Demonstrates Java synchronized blocks (monitorenter / monitorexit).
 *
 * <p>On real hardware with threads, synchronized prevents data races. In the simulator
 * (single-threaded), it verifies the bytecodes are handled without error and reentrant locking
 * works correctly.
 */
public class SyncDemo extends Application {
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
