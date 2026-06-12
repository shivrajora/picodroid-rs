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

    // 3. unbind, then stop. After stop, the service is destroyed (no clients,
    // not started). onDestroy + foreground-notification cancel both fire.
    unbindService(conn);
    stopService(new Intent(CounterService.class));

    Log.i("ServiceDemoApp", "end");
  }
}
