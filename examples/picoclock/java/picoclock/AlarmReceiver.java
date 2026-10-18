// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import picoclock.ui.RingActivity;
import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * What an alarm's {@code PendingIntent.getBroadcast} delivers to: Android's idiom for an alarm
 * clock, a receiver that brings up the ring screen. The framework starts the app first if it was
 * not running, and {@link AlarmService} is up by the time this runs (ClockApp starts it in {@code
 * onCreate}), so {@link RingActivity} can bind it and ring. The alarm's id and due minute ride
 * along as the Intent's extras, from the operation {@link AlarmService#plan} armed.
 */
public class AlarmReceiver extends BroadcastReceiver {
  public AlarmReceiver() {}

  @Override
  public void onReceive(Context context, Intent intent) {
    int id = intent.getIntExtra(RingActivity.EXTRA_ALARM_ID, -1);
    Log.i(ClockApp.TAG, "alarm " + id + " fired");
    context.startActivity(
        new Intent(RingActivity.class)
            .putExtra(RingActivity.EXTRA_ALARM_ID, id)
            .putExtra(
                RingActivity.EXTRA_DUE_MINUTE,
                intent.getIntExtra(RingActivity.EXTRA_DUE_MINUTE, 0)));
  }
}
