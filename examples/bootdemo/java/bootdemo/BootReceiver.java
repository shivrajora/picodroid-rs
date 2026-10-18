// SPDX-License-Identifier: GPL-3.0-only
package bootdemo;

import picodroid.app.AlarmManager;
import picodroid.app.PendingIntent;
import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/** Heard once per boot, and on a clock or zone set while the app is not running. */
public class BootReceiver extends BroadcastReceiver {
  /** How far out the ping is armed: long enough for the launcher to come up first. */
  static final int PING_MS = 4000;

  public BootReceiver() {}

  @Override
  public void onReceive(Context context, Intent intent) {
    String action = intent.getAction();
    if (Intent.ACTION_BOOT_COMPLETED.equals(action)) {
      Log.i(BootDemoApp.TAG, "boot completed");
    } else {
      Log.i(BootDemoApp.TAG, "clock moved: " + action);
    }
    AlarmManager am = (AlarmManager) context.getSystemService(Context.ALARM_SERVICE);
    PendingIntent ping =
        PendingIntent.getBroadcast(
            context,
            1,
            new Intent(context, PingReceiver.class).putExtra("n", 7),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    am.setExact(
        AlarmManager.ELAPSED_REALTIME_WAKEUP, SystemClock.elapsedRealtime() + PING_MS, ping);
    Log.i(BootDemoApp.TAG, "ping armed");
  }
}
