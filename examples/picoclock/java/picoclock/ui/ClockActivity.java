// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.Alarm;
import picoclock.AlarmSchedule;
import picoclock.Clock;
import picoclock.ClockApp;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * The face: the time in large digits, today's date, what rings next, and the two places to go from
 * here. This is the app's root, so BACK on it leaves for the launcher.
 *
 * <p>The face repaints off {@link ClockViewModel#now}, which ticks only while this screen is
 * started.
 */
public class ClockActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  private SegmentClock face;
  private TextView seconds;
  private TextView date;
  private TextView next;

  private long lastDay = Long.MIN_VALUE;
  private int lastSecond = -1;
  private int lastMinute = -1;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    LinearLayout root = Ui.screen(this);
    Ui.header(root, "Clock", null);

    face = new SegmentClock(root, 52);
    seconds = Ui.centred(root, "", 17, Ui.MUTED);
    date = Ui.centred(root, "", 16, Ui.TEXT);
    next = Ui.centred(root, "", 16, Ui.ACCENT);
    Ui.fill(root);

    Button alarmsButton = Ui.button(this, "Alarms", Ui.ACCENT);
    alarmsButton.setOnClickListener(v -> startActivity(new Intent(this, AlarmListActivity.class)));
    Button setButton = Ui.button(this, "Set time", Ui.SURFACE_HIGH);
    setButton.setOnClickListener(v -> startActivity(new Intent(this, SetTimeActivity.class)));
    Ui.row(root, Ui.TAP_HEIGHT, Ui.MARGIN, alarmsButton, setButton);

    // A clock face is read from across the room: hold the panel on while it shows. The
    // screens behind it (alarms, set time) let the display doze on the board's timeout.
    root.setKeepScreenOn(true);
    setContentView(root);

    viewModel(ClockViewModel.class).now().observe(this, this::redraw);
  }

  @Override
  public void onResume() {
    super.onResume();
    // Back from an edit or a clock set: every cached line below may be stale.
    lastSecond = -1;
    lastMinute = -1;
    lastDay = Long.MIN_VALUE;
  }

  @Override
  protected void onAlarmsReady() {
    lastMinute = -1;
  }

  // See BaseActivity: a lifecycle callback is only reached when the concrete
  // class declares it.
  @Override
  public void onPause() {
    super.onPause();
  }

  @Override
  public void onDestroy() {
    super.onDestroy();
  }

  /**
   * Repaint whatever changed since the last tick and nothing else. The face compares its own
   * segments, and each line below is rebuilt only when the quantity behind it moved — the date once
   * a day, the countdown once a minute — so a tick that changes nothing allocates nothing.
   */
  private void redraw(long utc) {
    boolean clockSet = Clock.isSet(utc);
    long local = Clock.toLocal(utc, store.offsetMinutes());

    // The colon blinks on the half second, which is why the face is repainted
    // on every tick and not only when the displayed minute changes.
    face.show(
        Clock.hourOf(local),
        Clock.minuteOf(local),
        Clock.msIntoDay(local) % Clock.MS_PER_SECOND < 500);

    int second = Clock.secondOf(local);
    if (second != lastSecond) {
      lastSecond = second;
      seconds.setText(
          clockSet
              ? ":" + Clock.two(second) + "   " + Clock.offset(store.offsetMinutes())
              : "clock not set - tap Set time");
    }

    long day = Clock.dayOf(local);
    if (day != lastDay) {
      lastDay = day;
      date.setText(clockSet ? Clock.date(local) : "");
    }

    int minute = Clock.minuteOf(local);
    if (minute != lastMinute) {
      lastMinute = minute;
      String upcoming = nextText(utc);
      next.setText(upcoming);
      Log.i(TAG, "next: " + upcoming);
    }
  }

  private String nextText(long utc) {
    if (alarms == null) {
      return "";
    }
    long at = alarms.nextFireUtcMs();
    if (at == AlarmSchedule.NEVER) {
      return "No alarm set";
    }
    Alarm a = alarms.nextAlarm();
    long localFire = Clock.toLocal(at, store.offsetMinutes());
    String when = Clock.hm(Clock.hourOf(localFire), Clock.minuteOf(localFire));
    String label = a == null || a.label.isEmpty() ? "" : " " + a.label;
    return when + label + "  " + Clock.until(at - utc);
  }
}
