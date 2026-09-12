// SPDX-License-Identifier: GPL-3.0-only
package alarmdemo;

import picodroid.app.AlarmManager;
import picodroid.app.Application;
import picodroid.app.PendingIntent;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * What an {@link AlarmManager} alarm is for: the app sets one, leaves, and is started again to
 * receive it.
 *
 * <p>The app arms an alarm three seconds out and then, if this board has a launcher to go to, walks
 * out of itself. The framework holds the alarm while nothing of this app is running, starts it
 * again when the alarm comes due, and delivers it to {@link WokeActivity}.
 *
 * <p>Where there is no launcher — the harness runs one app on its own — leaving would end the run
 * before the alarm could fire, so it stays up instead and the alarm arrives in the same run. Both
 * paths end at the same log line, which is what the harness asserts on.
 */
public class AlarmDemoApp extends Application {
  public static final String TAG = "AlarmDemo";

  /** The alarm's id, carried as an extra and echoed back by {@link WokeActivity}. */
  public static final int ALARM_ID = 7;

  /** Tells the request code apart from the id; any int would do. */
  public static final int REQUEST_CODE = 1;

  static final String EXTRA_ID = "id";

  /** How far out the alarm is set: long enough to leave the app first. */
  private static final int DELAY_MS = 3000;

  /** Set once {@link WokeActivity} has run, so nothing tries to leave afterwards. */
  static volatile boolean woken;

  @Override
  public void onCreate() {
    AlarmManager alarms = (AlarmManager) getSystemService(Context.ALARM_SERVICE);
    alarms.setExact(
        AlarmManager.RTC_WAKEUP, System.currentTimeMillis() + DELAY_MS, operation(this));
    Log.i(TAG, "armed id=" + ALARM_ID);
    startActivity(new Intent(MainActivity.class));
  }

  /**
   * The operation the alarm carries, built the same way on both sides of the app switch: an equal
   * one cancels or replaces the alarm it was set with.
   */
  static PendingIntent operation(Context context) {
    return PendingIntent.getActivity(
        context,
        REQUEST_CODE,
        new Intent(WokeActivity.class).putExtra(EXTRA_ID, ALARM_ID),
        PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
  }
}
