// SPDX-License-Identifier: GPL-3.0-only
package tutorial_service;

import picodroid.app.Notification;
import picodroid.app.Service;
import picodroid.concurrent.Thread;
import picodroid.content.Intent;
import picodroid.os.IBinder;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Hardware-independent background Service that periodically records a monotonic timestamp into a
 * fixed-size ring buffer, so an Activity can later bind and read back the recent samples.
 *
 * <p>This Service is both <em>started</em> and <em>bindable</em>:
 *
 * <ul>
 *   <li><b>Started</b> (from {@link TutorialServiceApp#onCreate}) so it keeps sampling on its own
 *       background Thread and survives Activity navigation — a bound-only Service would be
 *       destroyed the moment its last Activity finishes, taking the ring buffer with it.
 *   <li><b>Bindable</b> so {@link LogViewerActivity} can grab a typed handle through the {@link
 *       LocalBinder} and copy out a snapshot of the samples collected so far.
 * </ul>
 */
public class UptimeLogService extends Service {
  private static final String TAG = "UptimeLogService";
  private static final int NOTIFICATION_ID = 1;

  /** Fixed ring-buffer capacity. The newest {@code CAPACITY} samples are retained. */
  public static final int CAPACITY = 16;

  /** How often the background Thread takes a sample, in milliseconds. */
  private static final int SAMPLE_INTERVAL_MS = 1000;

  /**
   * Typed handle handed to clients. Picodroid is single-process, so the LocalBinder simply carries
   * a direct reference to the Service — clients cast the {@link IBinder} they receive back to this
   * type. Mirrors the pattern in {@code servicedemo} and {@code picoenvmon}.
   */
  public static class LocalBinder implements IBinder {
    public UptimeLogService service;
  }

  private final LocalBinder binder = new LocalBinder();

  // Circular buffer of monotonic timestamps (ms). head = next write index; count saturates at
  // CAPACITY once the ring has wrapped. The sampler Thread writes them and the main thread reads
  // them in snapshot(), so every access is inside a synchronized block on `lock` — this both
  // publishes the writes across threads and makes each snapshot atomic with respect to a sample.
  private final Object lock = new Object();
  private final long[] samples = new long[CAPACITY];
  private int head;
  private int count;

  // Guards onStartCommand so repeated startService calls don't spawn a second sampler Thread or a
  // duplicate foreground banner — onStartCommand fires on EVERY startService, including repeats.
  private boolean started;
  private volatile boolean running;

  @Override
  public void onCreate() {
    // Wire the binder back to this instance up front so a bind that races the first start still
    // resolves to a live Service. onCreate runs once, on the first start OR the first bind.
    binder.service = this;
    Log.i(TAG, "onCreate");
  }

  @Override
  public int onStartCommand(Intent intent, int startId) {
    Log.i(TAG, "onStartCommand id=" + startId);
    if (!started) {
      started = true;
      running = true;

      Notification n =
          new Notification.Builder()
              .setContentTitle("Uptime Logger")
              .setContentText("Recording uptime samples")
              .build();
      startForeground(NOTIFICATION_ID, n);

      // Sampling is blocking work (it sleeps between samples), so it MUST run off the main thread.
      // picodroid.concurrent.Thread is the supported way to do that; SystemClock.sleep is the only
      // blocking sleep in the SDK and is safe to call here because we are not on the main thread.
      new Thread(this::sampleLoop).start();
      Log.i(TAG, "foreground started, sampler running");
    }
    // The return value is ignored on picodroid (the OS never kills a running Service); START_STICKY
    // is returned for source-level Android compatibility.
    return START_STICKY;
  }

  private void sampleLoop() {
    while (running) {
      recordSample();
      SystemClock.sleep(SAMPLE_INTERVAL_MS);
    }
  }

  private void recordSample() {
    // elapsedRealtimeNanos() is monotonic (it never jumps backwards), which is what we want for an
    // uptime log; convert to milliseconds for a compact, readable value.
    long uptimeMs = SystemClock.elapsedRealtimeNanos() / 1_000_000L;
    synchronized (lock) {
      samples[head] = uptimeMs;
      head = (head + 1) % CAPACITY;
      if (count < CAPACITY) {
        count++;
      }
    }
  }

  @Override
  public IBinder onBind(Intent intent) {
    Log.i(TAG, "onBind");
    return binder;
  }

  @Override
  public boolean onUnbind(Intent intent) {
    Log.i(TAG, "onUnbind");
    return false;
  }

  @Override
  public void onDestroy() {
    // Stop the sampler loop, drop the foreground banner. onDestroy runs only when the Service is
    // neither started nor bound — for a started Service that means after stopService/stopSelf with
    // no remaining clients.
    running = false;
    Log.i(TAG, "onDestroy count=" + count);
    stopForeground(true);
  }

  /**
   * Copy the recorded timestamps, oldest first, into {@code out}. Returns the number of samples
   * written (at most {@link #CAPACITY}). Synchronized on the same lock as the sampler, so the copy
   * is a consistent point-in-time view even while the background Thread is recording.
   */
  public int snapshot(long[] out) {
    synchronized (lock) {
      int n = count;
      int start = (head - n + CAPACITY) % CAPACITY;
      for (int i = 0; i < n && i < out.length; i++) {
        out[i] = samples[(start + i) % CAPACITY];
      }
      return n;
    }
  }
}
