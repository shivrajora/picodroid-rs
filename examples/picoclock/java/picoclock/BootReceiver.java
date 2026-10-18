// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * The manifest receiver Android's alarm clock has too: at boot, and when the clock or the zone is
 * set while the app is not running, the framework runs the app headless and this starts {@link
 * AlarmService}, whose {@code onCreate} re-plans every armed alarm with the framework. Nothing is
 * shown; the run ends once the service has been handed its alarms.
 */
public class BootReceiver extends BroadcastReceiver {
  public BootReceiver() {}

  @Override
  public void onReceive(Context context, Intent intent) {
    Log.i(ClockApp.TAG, "re-arming alarms: " + intent.getAction());
    context.startService(new Intent(context, AlarmService.class));
  }
}
