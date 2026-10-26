// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.Clock;
import picodroid.os.Bundle;
import picodroid.view.Gravity;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.DatePicker;
import picodroid.widget.LinearLayout;
import picodroid.widget.RelativeLayout;
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
 * <p>The fields live in {@link SetTimeViewModel}; every control writes to it and this screen paints
 * what it publishes.
 */
public class SetTimeActivity extends BaseActivity {
  private static final String[] MONTHS = {
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December"
  };

  /** Height of a stepper row, and the width of its two buttons. */
  private static final int STEP_HEIGHT = 44;

  private static final int STEP_BUTTON = 54;

  private static final int MATCH = ViewGroup.LayoutParams.MATCH_PARENT;

  private SetTimeViewModel model;

  private DatePicker calendar;
  private TimePicker time;
  private TextView yearLabel;
  private TextView monthLabel;
  private TextView offsetLabel;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    model = viewModel(SetTimeViewModel.class);

    LinearLayout root = Ui.screen(this);
    Ui.header(root, "Set time", v -> finish());

    // Three pickers and three steppers do not fit on a 480-pixel panel.
    LinearLayout page = new LinearLayout(this);
    page.setOrientation(LinearLayout.VERTICAL);
    page.setPadding(0, 0, 0, 14);

    Ui.label(page, "Date", 6, Ui.MUTED);
    yearLabel = stepper(page, 8, v -> model.stepYear(-1), v -> model.stepYear(1));
    monthLabel = stepper(page, Ui.GAP, v -> model.stepMonth(-1), v -> model.stepMonth(1));

    calendar = new DatePicker(this);
    calendar.setOnDateChangedListener((view, y, m, d) -> model.pickDate(y, m + 1, d));
    page.addView(calendar, inset(200, 8));

    Ui.label(page, "Time", 12, Ui.MUTED);
    time = new TimePicker(this);
    time.setIs24HourView(true);
    time.setOnTimeChangedListener((view, h, m) -> model.pickTime(h, m));
    page.addView(time, inset(130, 10));

    Ui.label(page, "Time zone", 12, Ui.MUTED);
    offsetLabel = stepper(page, 8, v -> model.stepOffset(-1), v -> model.stepOffset(1));

    Button apply = Ui.button(this, "Set clock", Ui.ACCENT);
    apply.setOnClickListener(
        v -> {
          if (model.apply()) {
            finish();
          }
        });
    page.addView(apply, inset(Ui.TAP_HEIGHT, 16));

    Ui.label(page, "No battery-backed clock on this board:", 14, Ui.MUTED);
    Ui.label(page, "the time is lost on a power cut.", 6, Ui.MUTED);

    ScrollView scroller = new ScrollView(this);
    // A ScrollView arrives with the theme's surface; this one is part of the page.
    scroller.setBackgroundColor(Ui.BACKGROUND);
    scroller.addView(page, new ViewGroup.LayoutParams(MATCH, ViewGroup.LayoutParams.WRAP_CONTENT));
    root.addView(scroller, new LinearLayout.LayoutParams(MATCH, 0, 1f));

    setContentView(root);
    model.fields().observe(this, this::show);
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

  /** Paints the fields; a picker already showing the value is left alone, mid-gesture or not. */
  private void show(SetTimeViewModel.Fields f) {
    yearLabel.setText(String.valueOf(f.year()));
    monthLabel.setText(MONTHS[f.month() - 1]);
    offsetLabel.setText(Clock.offset(f.offsetMinutes));
    if (calendar.getYear() != f.year()
        || calendar.getMonth() != f.month() - 1
        || calendar.getDayOfMonth() != f.day()) {
      calendar.updateDate(f.year(), f.month() - 1, f.day());
    }
    if (time.getHour() != f.hour() || time.getMinute() != f.minute()) {
      time.setHour(f.hour());
      time.setMinute(f.minute());
    }
  }

  /** Full-width layout params, inset by the side margin, {@code top} below the view above. */
  private static LinearLayout.LayoutParams inset(int height, int top) {
    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(MATCH, height);
    lp.setMargins(Ui.MARGIN, top, Ui.MARGIN, 0);
    return lp;
  }

  // ── Steppers ───────────────────────────────────────────────────────────────

  /** The ids of a stepper's buttons, which the value's rules name. */
  private static final int ID_MINUS = 1;

  private static final int ID_PLUS = 2;

  /**
   * Adds a "- value +" row to {@code page}, {@code top} below the view above; returns the label
   * between the buttons. A RelativeLayout: the buttons at the row's edges, the value stretched
   * between them and centred on their height.
   */
  private TextView stepper(
      LinearLayout page, int top, View.OnClickListener down, View.OnClickListener up) {
    RelativeLayout row = new RelativeLayout(this);

    Button minus = Ui.button(this, "-", Ui.SURFACE_HIGH);
    minus.setId(ID_MINUS);
    minus.setOnClickListener(down);
    RelativeLayout.LayoutParams minusLp = new RelativeLayout.LayoutParams(STEP_BUTTON, STEP_HEIGHT);
    minusLp.addRule(RelativeLayout.ALIGN_PARENT_LEFT);
    minusLp.setMargins(Ui.MARGIN, 0, 0, 0);
    row.addView(minus, minusLp);

    Button plus = Ui.button(this, "+", Ui.SURFACE_HIGH);
    plus.setId(ID_PLUS);
    plus.setOnClickListener(up);
    RelativeLayout.LayoutParams plusLp = new RelativeLayout.LayoutParams(STEP_BUTTON, STEP_HEIGHT);
    plusLp.addRule(RelativeLayout.ALIGN_PARENT_RIGHT);
    plusLp.setMargins(0, 0, Ui.MARGIN, 0);
    row.addView(plus, plusLp);

    TextView label = Ui.line(this, "", Ui.TEXT);
    label.setGravity(Gravity.CENTER_HORIZONTAL);
    RelativeLayout.LayoutParams labelLp =
        new RelativeLayout.LayoutParams(MATCH, ViewGroup.LayoutParams.WRAP_CONTENT);
    labelLp.addRule(RelativeLayout.RIGHT_OF, ID_MINUS);
    labelLp.addRule(RelativeLayout.LEFT_OF, ID_PLUS);
    labelLp.addRule(RelativeLayout.CENTER_VERTICAL);
    labelLp.setMargins(Ui.GAP, 0, Ui.GAP, 0);
    row.addView(label, labelLp);

    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(MATCH, STEP_HEIGHT);
    lp.topMargin = top;
    page.addView(row, lp);
    return label;
  }
}
