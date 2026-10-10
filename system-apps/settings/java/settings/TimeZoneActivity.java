// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.TimeZone;
import picodroid.app.Activity;
import picodroid.app.AlarmManager;
import picodroid.content.Context;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.View;

/**
 * The time zone list: one row per fixed offset, UTC-12:00 to UTC+14:00, whole hours plus the
 * fractional ones in use, the current zone marked. A tap (or SELECT) stores the pick through {@link
 * AlarmManager#setTimeZone} and returns to Date &amp; time. There is no tz database on the device
 * (picoclock-roadmap R4), so a zone is an offset, never a region, and daylight saving is the user
 * moving it twice a year. The pick is logged as {@code time zone <id>}.
 */
public class TimeZoneActivity extends Activity {
  private static final String TAG = SettingsActivity.TAG;

  /** Minutes east of UTC, in list order. */
  static final int[] OFFSETS = {
    -720, -660, -600, -570, -540, -480, -420, -360, -300, -240, -210, -180, -120, -60, 0, 60, 120,
    180, 210, 240, 270, 300, 330, 345, 360, 390, 420, 480, 525, 540, 570, 600, 630, 660, 720, 765,
    780, 840
  };

  private Column column;
  private final View[] rows = new View[OFFSETS.length];
  private int current;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    int minutes = TimeZone.getDefault().getRawOffset() / 60_000;
    current = indexOf(minutes);
    column = new Column(this, "Time zone", v -> finish());
    column.fill(null, i -> row(i), () -> rows[current].requestFocus());
  }

  private View row(int i) {
    if (i >= OFFSETS.length) {
      return null;
    }
    final String id = id(OFFSETS[i]);
    rows[i] =
        i == current
            ? Screens.row(this, id, "current", v -> pick(id))
            : Screens.row(this, id, v -> pick(id));
    return rows[i];
  }

  private void pick(String id) {
    AlarmManager am = (AlarmManager) getSystemService(Context.ALARM_SERVICE);
    am.setTimeZone(id);
    Log.i(TAG, "time zone " + id);
    finish();
  }

  /** The id {@code TimeZone.getDefault()} will report for {@code minutes}: UTC, or GMT±hh:mm. */
  static String id(int minutes) {
    if (minutes == 0) {
      return "UTC";
    }
    int abs = minutes < 0 ? -minutes : minutes;
    int h = abs / 60;
    int m = abs % 60;
    return (minutes < 0 ? "GMT-" : "GMT+") + (h < 10 ? "0" : "") + h + (m < 10 ? ":0" : ":") + m;
  }

  private static int indexOf(int minutes) {
    for (int i = 0; i < OFFSETS.length; i++) {
      if (OFFSETS[i] == minutes) {
        return i;
      }
    }
    return indexOf(0);
  }

  @Override
  public void onDestroy() {
    column.stop();
    super.onDestroy();
  }
}
