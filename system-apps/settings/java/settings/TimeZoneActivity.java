// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.TimeZone;
import picodroid.app.Activity;
import picodroid.app.AlarmManager;
import picodroid.content.Context;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;

/**
 * The time zone list: one entry per fixed offset, UTC-12:00 to UTC+14:00, whole hours plus the
 * fractional ones in use, the current zone marked. A tap (or SELECT) stores the pick through {@link
 * AlarmManager#setTimeZone} and returns to Date &amp; time. There is no tz database on the device
 * (picoclock-roadmap R4), so a zone is an offset, never a region, and daylight saving is the user
 * moving it twice a year. The pick is logged as {@code time zone <id>}.
 *
 * <p>A {@link ListView} under the header rather than a {@link Column} of rows: 38 rows each with a
 * click listener of their own would overrun the framework's click-listener table (32), and a list
 * is the Android shape for a pick anyway.
 */
public class TimeZoneActivity extends Activity {
  private static final String TAG = SettingsActivity.TAG;

  /** Minutes east of UTC, in list order. */
  static final int[] OFFSETS = {
    -720, -660, -600, -570, -540, -480, -420, -360, -300, -240, -210, -180, -120, -60, 0, 60, 120,
    180, 210, 240, 270, 300, 330, 345, 360, 390, 420, 480, 525, 540, 570, 600, 630, 660, 720, 765,
    780, 840
  };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    int current = TimeZone.getDefault().getRawOffset() / 60_000;
    String[] labels = new String[OFFSETS.length];
    for (int i = 0; i < OFFSETS.length; i++) {
      labels[i] = OFFSETS[i] == current ? id(OFFSETS[i]) + "  (current)" : id(OFFSETS[i]);
    }
    LinearLayout root = Screens.column(this);
    root.addView(Screens.header(this, "Time zone", v -> finish()));
    ListView list = new ListView(this);
    list.setSize(getDisplay().getWidth(), getDisplay().getHeight() - Screens.ROW_HEIGHT);
    list.setAdapter(new ArrayAdapter<String>(this, labels));
    list.setOnItemClickListener((parent, view, position, id) -> pick(id(OFFSETS[position])));
    root.addView(list);
    setContentView(root);
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
}
