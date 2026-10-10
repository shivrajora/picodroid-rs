// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.TimeZone;
import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.provider.Settings;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.TextView;

/**
 * Date &amp; time, as Android's page of that name carries it: "Automatic date &amp; time" (the
 * platform time service anchors the clock from the network; {@link Settings.Global#AUTO_TIME}), the
 * time zone ({@link TimeZoneActivity}, stored through {@code AlarmManager.setTimeZone} and read by
 * every app through {@code TimeZone.getDefault()}), and the time the device shows now, or "not set"
 * while the clock has never been anchored this boot. There is no manual set here: a board with a
 * network gets its time from the network, and the one app that needs a hand-set clock on a board
 * without one ({@code picoclock}) keeps its own screen for that. The toggle is logged as {@code
 * date-time auto <0|1>}; the rows being in as {@code date-time}.
 */
public class DateTimeActivity extends Activity {
  private static final String TAG = SettingsActivity.TAG;

  /** Epoch ms of 2001-01-01: below it the clock has not been anchored this boot. */
  private static final long SET_THRESHOLD_MS = 978_307_200_000L;

  private Column column;
  private final View[] rows = new View[3];
  private TextView autoTail;
  private TextView zoneTail;
  private TextView timeTail;
  private boolean built;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    column = new Column(this, "Date & time", v -> finish());
    column.fill(null, i -> row(i), () -> ready());
  }

  private View row(int i) {
    switch (i) {
      case 0:
        autoTail = Screens.tail(this, autoOn() ? "On" : "Off");
        rows[0] = Screens.info(this, "Automatic date & time", autoTail);
        rows[0].setOnClickListener(v -> toggleAuto());
        return rows[0];
      case 1:
        zoneTail = Screens.tail(this, TimeZone.getDefault().getID());
        rows[1] = Screens.info(this, "Time zone", zoneTail);
        rows[1].setOnClickListener(v -> startActivity(new Intent(TimeZoneActivity.class)));
        return rows[1];
      case 2:
        timeTail = Screens.tail(this, timeNow());
        rows[2] = Screens.info(this, "Time", timeTail);
        return rows[2];
      default:
        return null;
    }
  }

  private void ready() {
    rows[0].requestFocus();
    built = true;
    Log.i(TAG, "date-time");
  }

  /** Back from the zone list: the zone row and the time follow the pick. */
  @Override
  public void onResume() {
    super.onResume();
    if (built) {
      zoneTail.setText(TimeZone.getDefault().getID());
      timeTail.setText(timeNow());
    }
  }

  private boolean autoOn() {
    return Settings.Global.getInt(getContentResolver(), Settings.Global.AUTO_TIME, 1) != 0;
  }

  private void toggleAuto() {
    int next = autoOn() ? 0 : 1;
    boolean ok = Settings.Global.putInt(getContentResolver(), Settings.Global.AUTO_TIME, next);
    Log.i(TAG, "date-time auto " + next + (ok ? "" : " (not stored)"));
    autoTail.setText(next != 0 ? "On" : "Off");
  }

  /** "14:03" in the platform zone, or "not set". Integer arithmetic: no java.time on every board. */
  private static String timeNow() {
    long utc = System.currentTimeMillis();
    if (utc < SET_THRESHOLD_MS) {
      return "not set";
    }
    long local = utc + TimeZone.getDefault().getRawOffset();
    long minutes = Math.floorDiv(local, 60_000L);
    long hh = Math.floorMod(minutes / 60, 24L);
    long mm = Math.floorMod(minutes, 60L);
    return (hh < 10 ? "0" : "") + hh + (mm < 10 ? ":0" : ":") + mm;
  }

  @Override
  public void onDestroy() {
    column.stop();
    super.onDestroy();
  }
}
