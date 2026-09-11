// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import javax.inject.Inject;
import picoclock.AlarmStore;
import picoclock.Clock;
import picoclock.ClockApp;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.DatePicker;
import picodroid.widget.FrameLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;
import picodroid.widget.TimePicker;

/**
 * Setting the wall clock, which on this board is not optional: the RP2350B has no battery-backed
 * RTC and the carrier adds none, so every cold boot starts the clock at the epoch and an alarm has
 * nothing to fire against until someone says what time it is.
 *
 * <p>Year and month are steppers of this screen's own. The SDK's {@link DatePicker} is a calendar
 * of one month with no header to move off it, so on its own it can choose a day and nothing else —
 * the steppers are what make a year reachable, and they rewrite the calendar under them as they
 * move.
 *
 * <p>{@link SystemClock#setCurrentTimeMillis} takes UTC, so what the pickers show is shifted by the
 * offset on the way in and on the way out: change the zone and the fields follow it, keeping the
 * same instant.
 */
public class SetTimeActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  /**
   * Where the fields start when the clock has never been set. Not "now" — there is no now to read —
   * and not 1970 either, which {@link Clock#isSet} would treat as still unset and which is a long
   * way from anywhere the user wants to be. A round recent date is the shortest walk to a real one.
   */
  private static final int DEFAULT_YEAR = 2026;

  /** The offsets the steppers walk, in minutes: whole hours, plus the fractional ones in use. */
  private static final int[] OFFSETS = {
    -720, -660, -600, -540, -480, -420, -360, -300, -240, -210, -180, -120, -60, 0, 60, 120, 180,
    210, 240, 270, 300, 330, 345, 360, 390, 420, 480, 540, 570, 600, 630, 660, 720, 780, 840
  };

  private static final String[] MONTHS = {
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December"
  };

  /** Height of a stepper row, and the width of its two buttons. */
  private static final int STEP_HEIGHT = 44;

  private static final int STEP_BUTTON = 54;

  private static final int PAGE_HEIGHT = 720;

  @Inject AlarmStore store;

  private DatePicker calendar;
  private TimePicker time;
  private TextView yearLabel;
  private TextView monthLabel;
  private TextView offsetLabel;

  private int year;
  private int month;

  /**
   * The chosen day of the month. Tracked here rather than read back from the calendar: the SDK's
   * DatePicker reports the most-recently-*pressed* cell and returns zero until one is, so a user
   * who accepts the day already showing would otherwise set the clock to the day before the month
   * began.
   */
  private int dayOfMonth;

  private int offsetMinutes;

  @Override
  public void onCreate() {
    super.onCreate();
    offsetMinutes = store.offsetMinutes();

    long utc = System.currentTimeMillis();
    boolean known = Clock.isSet(utc);
    long local = Clock.toLocal(utc, offsetMinutes);
    int[] ymd = known ? Clock.civilFromDay(Clock.dayOf(local)) : new int[] {DEFAULT_YEAR, 1, 1};
    year = ymd[0];
    month = ymd[1];
    dayOfMonth = ymd[2];

    FrameLayout root = Ui.screen();
    root.addView(Ui.header(this, "Set time", v -> finish()));

    // Three pickers and three steppers do not fit on a 480-pixel panel.
    FrameLayout page = Ui.group(0, 0, Ui.WIDTH, PAGE_HEIGHT);

    page.addView(Ui.label("Date", Ui.MARGIN, 6, Ui.MUTED));
    yearLabel = stepper(page, 30, "" + year, v -> stepYear(-1), v -> stepYear(1));
    monthLabel =
        stepper(
            page,
            30 + STEP_HEIGHT + Ui.GAP,
            MONTHS[month - 1],
            v -> stepMonth(-1),
            v -> stepMonth(1));

    calendar = new DatePicker();
    calendar.setSize(Ui.WIDTH - 2 * Ui.MARGIN, 200);
    calendar.setPosition(Ui.MARGIN, 30 + 2 * (STEP_HEIGHT + Ui.GAP));
    calendar.setDate(year, month, dayOfMonth);
    calendar.setOnDateChangedListener((view, y, m, d) -> dayOfMonth = d);
    page.addView(calendar);

    page.addView(Ui.label("Time", Ui.MARGIN, 346, Ui.MUTED));
    time = new TimePicker();
    time.setSize(Ui.WIDTH - 2 * Ui.MARGIN, 130);
    time.setPosition(Ui.MARGIN, 372);
    time.setIs24HourView(true);
    time.setTime(known ? Clock.hourOf(local) : 0, known ? Clock.minuteOf(local) : 0);
    page.addView(time);

    page.addView(Ui.label("Time zone", Ui.MARGIN, 514, Ui.MUTED));
    offsetLabel =
        stepper(page, 538, Clock.offset(offsetMinutes), v -> stepOffset(-1), v -> stepOffset(1));

    View apply = Ui.button("Set clock", Ui.MARGIN, 598, Ui.WIDTH - 2 * Ui.MARGIN, Ui.ACCENT);
    apply.setOnClickListener(v -> apply());
    page.addView(apply);

    page.addView(Ui.label("No battery-backed clock on this board:", Ui.MARGIN, 668, Ui.MUTED));
    page.addView(Ui.label("the time is lost on a power cut.", Ui.MARGIN, 690, Ui.MUTED));

    ScrollView scroller = new ScrollView();
    scroller.setSize(Ui.WIDTH, Ui.HEIGHT - Ui.HEADER_HEIGHT);
    scroller.setPosition(0, Ui.HEADER_HEIGHT);
    scroller.setPadding(0, 0, 0, 0);
    scroller.addView(page);
    root.addView(scroller);

    setContentView(root);
  }

  // See BaseActivity: a lifecycle callback is only reached when the concrete
  // class declares it.
  @Override
  public void onResume() {
    super.onResume();
  }

  @Override
  public void onPause() {
    super.onPause();
  }

  @Override
  public void onDestroy() {
    super.onDestroy();
  }

  // ── Steppers ───────────────────────────────────────────────────────────────

  /** A "- value +" row at {@code y}; returns the label between the buttons. */
  private TextView stepper(
      FrameLayout page, int y, String value, View.OnClickListener down, View.OnClickListener up) {
    Button minus = Ui.button("-", Ui.MARGIN, y, STEP_BUTTON, Ui.SURFACE_HIGH);
    minus.setSize(STEP_BUTTON, STEP_HEIGHT);
    minus.setOnClickListener(down);
    page.addView(minus);

    Button plus =
        Ui.button("+", Ui.WIDTH - Ui.MARGIN - STEP_BUTTON, y, STEP_BUTTON, Ui.SURFACE_HIGH);
    plus.setSize(STEP_BUTTON, STEP_HEIGHT);
    plus.setOnClickListener(up);
    page.addView(plus);

    int gapX = Ui.MARGIN + STEP_BUTTON + Ui.GAP;
    int gapWidth = Ui.WIDTH - 2 * gapX;
    return Ui.centred(page, value, gapX, y + (STEP_HEIGHT - Ui.LINE_HEIGHT) / 2, gapWidth, Ui.TEXT);
  }

  private void stepYear(int direction) {
    year += direction;
    yearLabel.setText("" + year);
    reDate();
  }

  private void stepMonth(int direction) {
    month += direction;
    if (month > 12) {
      month = 1;
      year++;
      yearLabel.setText("" + year);
    } else if (month < 1) {
      month = 12;
      year--;
      yearLabel.setText("" + year);
    }
    monthLabel.setText(MONTHS[month - 1]);
    reDate();
  }

  /** Re-point the calendar at the chosen year and month, clamping the day into it. */
  private void reDate() {
    int last = Clock.daysInMonth(year, month);
    if (dayOfMonth > last) {
      dayOfMonth = last;
    }
    calendar.setDate(year, month, dayOfMonth);
  }

  /**
   * Move one step along {@link #OFFSETS}, carrying the fields with it so the wall time on screen
   * stays the same instant while its label changes.
   */
  private void stepOffset(int direction) {
    int i = indexOfOffset(offsetMinutes) + direction;
    if (i < 0 || i >= OFFSETS.length) {
      return;
    }
    int was = offsetMinutes;
    offsetMinutes = OFFSETS[i];
    offsetLabel.setText(Clock.offset(offsetMinutes));

    long moved = pickedLocalMs() + (offsetMinutes - was) * Clock.MS_PER_MINUTE;
    int[] ymd = Clock.civilFromDay(Clock.dayOf(moved));
    year = ymd[0];
    month = ymd[1];
    dayOfMonth = ymd[2];
    yearLabel.setText("" + year);
    monthLabel.setText(MONTHS[month - 1]);
    calendar.setDate(year, month, dayOfMonth);
    time.setTime(Clock.hourOf(moved), Clock.minuteOf(moved));
  }

  private int indexOfOffset(int minutes) {
    for (int i = 0; i < OFFSETS.length; i++) {
      if (OFFSETS[i] == minutes) {
        return i;
      }
    }
    return 0;
  }

  // ── Applying ───────────────────────────────────────────────────────────────

  /** What the fields currently read, as local epoch ms. Seconds start at zero. */
  private long pickedLocalMs() {
    long epochDay = Clock.dayFromCivil(year, month, dayOfMonth);
    return epochDay * Clock.MS_PER_DAY
        + time.getHour() * Clock.MS_PER_HOUR
        + time.getMinute() * Clock.MS_PER_MINUTE;
  }

  private void apply() {
    long local = pickedLocalMs();
    store.setOffsetMinutes(offsetMinutes);
    if (!SystemClock.setCurrentTimeMillis(Clock.toUtc(local, offsetMinutes))) {
      Log.w(TAG, "the platform refused the clock set");
      return;
    }
    // Every armed alarm was scheduled against the old clock; none of those
    // instants means anything now.
    if (alarms != null) {
      alarms.reload();
    }
    Log.i(
        TAG,
        "clock set to "
            + Clock.date(local)
            + " "
            + Clock.hm(Clock.hourOf(local), Clock.minuteOf(local))
            + " "
            + Clock.offset(offsetMinutes));
    finish();
  }
}
