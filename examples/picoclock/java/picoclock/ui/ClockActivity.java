// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import javax.inject.Inject;
import picoclock.Alarm;
import picoclock.AlarmSchedule;
import picoclock.AlarmStore;
import picoclock.Clock;
import picoclock.ClockApp;
import picodroid.content.Intent;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.FrameLayout;
import picodroid.widget.TextView;

/**
 * The face: the time in large digits, today's date, what rings next, and the two places to go from
 * here. This is the app's root, so BACK on it leaves for the launcher.
 *
 * <p>Nothing here owns a clock of its own. The platform has no {@code postDelayed}, no timer and no
 * scheduled executor, and the documented ways to get a periodic callback — a thread that sleeps and
 * posts, or an animation's end action — would each cost a thread or an animation slot per screen.
 * {@link picoclock.AlarmService} already ticks four times a second to watch the alarms, so this
 * screen redraws off that heartbeat and adds nothing.
 */
public class ClockActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  @Inject AlarmStore store;

  private SegmentClock face;
  private TextView seconds;
  private TextView date;
  private TextView next;

  private long lastDay = Long.MIN_VALUE;
  private int lastSecond = -1;
  private int lastMinute = -1;

  @Override
  public void onCreate() {
    super.onCreate();
    FrameLayout root = Ui.screen();
    root.addView(Ui.header(this, "Clock", null));

    int faceTop = 96;
    face = new SegmentClock(root, SegmentClock.centredX(), faceTop);

    seconds = Ui.centred(root, "", faceTop + SegmentClock.height() + 14, Ui.MUTED);
    date = Ui.centred(root, "", faceTop + SegmentClock.height() + 46, Ui.TEXT);
    next = Ui.centred(root, "", faceTop + SegmentClock.height() + 78, Ui.ACCENT);

    int buttonsY = Ui.HEIGHT - Ui.TAP_HEIGHT - Ui.MARGIN;
    View alarmsButton =
        Ui.button("Alarms", Ui.columnX(0, 2), buttonsY, Ui.columnWidth(2), Ui.ACCENT);
    alarmsButton.setOnClickListener(v -> startActivity(new Intent(AlarmListActivity.class)));
    root.addView(alarmsButton);

    View setButton =
        Ui.button("Set time", Ui.columnX(1, 2), buttonsY, Ui.columnWidth(2), Ui.SURFACE_HIGH);
    setButton.setOnClickListener(v -> startActivity(new Intent(SetTimeActivity.class)));
    root.addView(setButton);

    setContentView(root);
    redraw();
  }

  @Override
  public void onResume() {
    super.onResume();
    // Back from an edit: the service still holds the alarms it read before it,
    // and every cached line below is stale.
    if (alarms != null) {
      alarms.reload();
    }
    lastSecond = -1;
    lastMinute = -1;
    lastDay = Long.MIN_VALUE;
    redraw();
  }

  @Override
  protected void onAlarmsReady() {
    lastMinute = -1;
    redraw();
  }

  /** The service's heartbeat, four times a second, and the only thing that drives the face. */
  @Override
  public void onTick() {
    redraw();
  }

  // See BaseActivity: a lifecycle callback is only reached when the concrete
  // class declares it.
  @Override
  public void onPause() {
    super.onPause();
  }

  @Override
  public void onDestroy() {
    face.stop();
    super.onDestroy();
  }

  /**
   * Repaint whatever changed since the last tick and nothing else. The face compares its own
   * segments, and each line below is rebuilt only when the quantity behind it moved — the date once
   * a day, the countdown once a minute — so a tick that changes nothing allocates nothing.
   */
  private void redraw() {
    long utc = System.currentTimeMillis();
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
