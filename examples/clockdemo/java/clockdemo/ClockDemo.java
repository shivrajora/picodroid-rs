package clockdemo;

import picodroid.app.Application;
import picodroid.os.SystemClock;
import picodroid.util.Log;

public class ClockDemo extends Application {
  private static final String TAG = "ClockDemo";

  public void onCreate() {
    long ms = System.currentTimeMillis();
    long ns = SystemClock.elapsedRealtimeNanos();
    Log.i(TAG, "currentTimeMillis = " + ms);
    Log.i(TAG, "elapsedRealtimeNanos = " + ns);
    // Sanity: ms should be roughly ns/1_000_000 (within a few ms of drift
    // since the two reads aren't atomic). Print the difference.
    long diffMs = ms - ns / 1_000_000L;
    Log.i(TAG, "ms - ns/1e6 = " + diffMs);
  }
}
