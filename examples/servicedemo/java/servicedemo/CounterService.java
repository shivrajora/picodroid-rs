// SPDX-License-Identifier: GPL-3.0-only
package servicedemo;

import picodroid.app.Notification;
import picodroid.app.Service;
import picodroid.content.Intent;
import picodroid.os.IBinder;
import picodroid.util.Log;

public class CounterService extends Service {
  public static class LocalBinder implements IBinder {
    public CounterService service;
  }

  private final LocalBinder binder = new LocalBinder();
  private int count = 0;
  private int startCount = 0;

  @Override
  public void onCreate() {
    binder.service = this;
    Log.i("CounterService", "onCreate");
  }

  @Override
  public int onStartCommand(Intent intent, int startId) {
    startCount++;
    int step = (intent != null) ? intent.getIntExtra("step", 1) : 1;
    count += step;
    Log.i("CounterService", "onStartCommand id=" + startId + " step=" + step + " count=" + count);
    if (startCount == 1) {
      Notification n =
          new Notification.Builder().setContentTitle("Counter").setContentText("running").build();
      startForeground(1, n);
    }
    return START_STICKY;
  }

  @Override
  public IBinder onBind(Intent intent) {
    Log.i("CounterService", "onBind");
    return binder;
  }

  @Override
  public boolean onUnbind(Intent intent) {
    Log.i("CounterService", "onUnbind");
    // Request onRebind on the next bind instead of a fresh onBind.
    return true;
  }

  @Override
  public void onRebind(Intent intent) {
    Log.i("CounterService", "onRebind");
  }

  @Override
  public void onDestroy() {
    Log.i("CounterService", "onDestroy count=" + count);
    stopForeground(true);
  }

  public int peek() {
    return count;
  }

  /** Exposes stopSelfResult to the bound client for the E2 demo. */
  public boolean tryStop(int startId) {
    return stopSelfResult(startId);
  }
}
