// SPDX-License-Identifier: GPL-3.0-only
package servicedemo;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.os.IBinder;
import picodroid.util.Log;

/**
 * Drives a CounterService through start, repeated start, bind (with peek), unbind, and stop —
 * exercising every Service v1 lifecycle path in one non-UI run.
 */
public class ServiceDemoApp extends Application {
  @Override
  public void onCreate() {
    Log.i("ServiceDemoApp", "begin");

    // 1. startService twice — onCreate fires once, onStartCommand fires twice.
    startService(new Intent(CounterService.class).putExtra("step", 5));
    startService(new Intent(CounterService.class).putExtra("step", 7));

    // 2. bindService — onBind fires, then onServiceConnected on the connection.
    ServiceConnection conn =
        new ServiceConnection() {
          @Override
          public void onServiceConnected(IBinder binder) {
            CounterService svc = ((CounterService.LocalBinder) binder).service;
            Log.i("ServiceDemoApp", "bound, peek=" + svc.peek());
          }

          @Override
          public void onServiceDisconnected() {
            Log.i("ServiceDemoApp", "disconnected");
          }
        };
    bindService(new Intent(CounterService.class), conn);

    // 3. unbind — onUnbind returns true, so the next bind triggers onRebind
    // (not onBind). The Service stays alive (still started).
    unbindService(conn);

    // 4. bind again — onRebind fires, the cached binder is reused. In
    // onServiceConnected, exercise stopSelfResult: a stale startId keeps the
    // Service running (false), the latest one stops it (true).
    ServiceConnection conn2 =
        new ServiceConnection() {
          @Override
          public void onServiceConnected(IBinder binder) {
            CounterService svc = ((CounterService.LocalBinder) binder).service;
            Log.i("ServiceDemoApp", "stale=" + svc.tryStop(1));
            Log.i("ServiceDemoApp", "latest=" + svc.tryStop(2));
          }

          @Override
          public void onServiceDisconnected() {}
        };
    bindService(new Intent(CounterService.class), conn2);
    unbindService(conn2);

    Log.i("ServiceDemoApp", "end");
  }
}
