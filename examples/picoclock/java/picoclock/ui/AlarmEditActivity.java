// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.Alarm;
import picodroid.activity.OnBackPressedCallback;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TimePicker;

/**
 * One alarm: its time on a {@link TimePicker}, the days it repeats on as seven toggles, and Save,
 * Delete and Cancel. Reached both for a new alarm and an existing one — the only difference is
 * whether Delete is offered. The draft, and what Cancel restores, live in {@link
 * AlarmEditViewModel}.
 */
public class AlarmEditActivity extends BaseActivity {
  /** The alarm slot to edit, as put by the list screen. */
  public static final String EXTRA_ID = "id";

  /** One-letter day headers, indexed by the bit position they carry in {@link Alarm#daysMask}. */
  private static final String[] DAY_INITIALS = {"S", "M", "T", "W", "T", "F", "S"};

  private static final int DAY_BUTTON = 38;
  private static final int DAY_GAP = 4;

  private AlarmEditViewModel model;

  private final Button[] dayButtons = new Button[7];

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Intent intent = getIntent();
    model = viewModel(AlarmEditViewModel.class);
    Alarm alarm = model.open(intent == null ? 0 : intent.getIntExtra(EXTRA_ID, 0));

    setTitle(model.existing() ? "Edit alarm" : "New alarm");
    LinearLayout root = Ui.screen(this);
    Ui.appBar(this, root, true);
    // BACK on this screen means Cancel, not "leave the half-made edit in place".
    getOnBackPressedDispatcher()
        .addCallback(
            this,
            new OnBackPressedCallback(true) {
              @Override
              public void handleOnBackPressed() {
                cancel();
              }
            });

    TimePicker picker = new TimePicker(this);
    picker.setIs24HourView(true);
    picker.setHour(alarm.hour);
    picker.setMinute(alarm.minute);
    picker.setOnTimeChangedListener((view, h, m) -> model.setTime(h, m));
    LinearLayout.LayoutParams pickerLp =
        new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 150);
    pickerLp.setMargins(Ui.MARGIN, 12, Ui.MARGIN, 0);
    root.addView(picker, pickerLp);

    Ui.label(root, "Repeat", 18, Ui.MUTED);
    buildDayToggles(root);
    model.days().observe(this, this::paintDays);
    Ui.fill(root);

    if (model.existing()) {
      Button delete = Ui.button(this, "Delete", Ui.SURFACE);
      delete.setTextColor(Ui.DANGER);
      delete.setOnClickListener(v -> delete());
      Ui.row(root, Ui.TAP_HEIGHT, Ui.GAP, delete);
    }

    Button save = Ui.button(this, "Save", Ui.ACCENT);
    save.setOnClickListener(v -> save());
    Button cancel = Ui.button(this, "Cancel", Ui.SURFACE_HIGH);
    cancel.setOnClickListener(v -> cancel());
    Ui.row(root, Ui.TAP_HEIGHT, Ui.MARGIN, save, cancel);

    setContentView(root);
  }

  /** Up means Cancel too. */
  @Override
  public boolean onNavigateUp() {
    cancel();
    return true;
  }

  /** Seven equal toggles across the screen, Monday first. */
  private void buildDayToggles(LinearLayout root) {
    LinearLayout row = new LinearLayout(this);
    row.setOrientation(LinearLayout.HORIZONTAL);
    // Monday-first, the way a week is read; the mask itself is Sunday-first
    // because that is what day-of-week arithmetic produces.
    for (int i = 0; i < 7; i++) {
      int day = (i + 1) % 7;
      Button b = new Button(this);
      b.setText(DAY_INITIALS[day]);
      b.setOnClickListener(v -> model.toggleDay(day));
      dayButtons[day] = b;
      LinearLayout.LayoutParams lp =
          new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f);
      if (i > 0) {
        lp.leftMargin = DAY_GAP;
      }
      row.addView(b, lp);
    }
    LinearLayout.LayoutParams lp =
        new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, DAY_BUTTON);
    lp.setMargins(Ui.MARGIN, 10, Ui.MARGIN, 0);
    root.addView(row, lp);
  }

  private void paintDays(int mask) {
    for (int day = 0; day < 7; day++) {
      boolean on = (mask & (1 << day)) != 0;
      Button b = dayButtons[day];
      b.setTextColor(on ? 0xFF000000 : Ui.MUTED);
      b.setBackground(Ui.rounded(on ? Ui.ACCENT : Ui.SURFACE, 8));
    }
  }

  private void save() {
    model.save();
    finish();
  }

  private void delete() {
    model.delete();
    finish();
  }

  private void cancel() {
    model.cancel();
    finish();
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
}
