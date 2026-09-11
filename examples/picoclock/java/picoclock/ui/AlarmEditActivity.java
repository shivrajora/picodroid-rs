// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import javax.inject.Inject;
import picoclock.Alarm;
import picoclock.AlarmStore;
import picoclock.ClockApp;
import picodroid.content.Intent;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.FrameLayout;
import picodroid.widget.TimePicker;

/**
 * One alarm: its time on a {@link TimePicker}, the days it repeats on as seven toggles, and Save,
 * Delete and Cancel. Reached both for a new alarm and an existing one — the only difference is
 * whether Delete is offered.
 *
 * <p>Edits land on the {@link Alarm} in the store immediately but are only written through on Save,
 * so Cancel has something to restore: the fields are snapshotted on entry and put back on the way
 * out. The store holds one object per slot for the life of the process, which is what makes that
 * snapshot three ints and a reference rather than a copy of anything.
 */
public class AlarmEditActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  /** The alarm slot to edit, as put by the list screen. */
  public static final String EXTRA_ID = "id";

  /** One-letter day headers, indexed by the bit position they carry in {@link Alarm#daysMask}. */
  private static final String[] DAY_INITIALS = {"S", "M", "T", "W", "T", "F", "S"};

  private static final int DAY_BUTTON = 38;
  private static final int DAY_GAP = 4;

  @Inject AlarmStore store;

  private Alarm alarm;
  private boolean existing;

  /** The alarm as it was on entry, for Cancel and for the BACK button. */
  private int wasHour;

  private int wasMinute;
  private int wasDays;
  private boolean wasEnabled;

  private final Button[] dayButtons = new Button[7];

  @Override
  public void onCreate() {
    super.onCreate();
    Intent intent = getIntent();
    int id = intent == null ? 0 : intent.getIntExtra(EXTRA_ID, 0);
    alarm = store.get(id);
    existing = store.exists(id);
    wasHour = alarm.hour;
    wasMinute = alarm.minute;
    wasDays = alarm.daysMask;
    wasEnabled = alarm.enabled;

    FrameLayout root = Ui.screen();
    root.addView(Ui.header(this, existing ? "Edit alarm" : "New alarm", v -> cancel()));

    TimePicker picker = new TimePicker();
    picker.setSize(Ui.WIDTH - 2 * Ui.MARGIN, 150);
    picker.setPosition(Ui.MARGIN, Ui.HEADER_HEIGHT + 12);
    picker.setIs24HourView(true);
    picker.setTime(alarm.hour, alarm.minute);
    picker.setOnTimeChangedListener(
        (view, h, m) -> {
          alarm.hour = h;
          alarm.minute = m;
        });
    root.addView(picker);

    int daysY = Ui.HEADER_HEIGHT + 180;
    root.addView(Ui.label("Repeat", Ui.MARGIN, daysY, Ui.MUTED));
    buildDayToggles(root, daysY + 26);

    int buttonsY = Ui.HEIGHT - Ui.TAP_HEIGHT - Ui.MARGIN;
    int deleteRowY = buttonsY - Ui.TAP_HEIGHT - Ui.GAP;

    View save = Ui.button("Save", Ui.columnX(0, 2), buttonsY, Ui.columnWidth(2), Ui.ACCENT);
    save.setOnClickListener(v -> save());
    root.addView(save);

    View cancel =
        Ui.button("Cancel", Ui.columnX(1, 2), buttonsY, Ui.columnWidth(2), Ui.SURFACE_HIGH);
    cancel.setOnClickListener(v -> cancel());
    root.addView(cancel);

    if (existing) {
      Button delete =
          Ui.button("Delete", Ui.MARGIN, deleteRowY, Ui.WIDTH - 2 * Ui.MARGIN, Ui.SURFACE);
      delete.setTextColor(Ui.DANGER);
      delete.setOnClickListener(v -> delete());
      root.addView(delete);
    }

    setContentView(root);
  }

  /** BACK on this screen means Cancel, not "leave the half-made edit in place". */
  @Override
  public void onBackPressed() {
    cancel();
  }

  private void buildDayToggles(FrameLayout root, int y) {
    // Monday-first, the way a week is read; the mask itself is Sunday-first
    // because that is what day-of-week arithmetic produces.
    int width = (Ui.WIDTH - 2 * Ui.MARGIN - 6 * DAY_GAP) / 7;
    for (int i = 0; i < 7; i++) {
      int day = (i + 1) % 7;
      Button b = new Button(DAY_INITIALS[day]);
      b.setSize(width, DAY_BUTTON);
      b.setPosition(Ui.MARGIN + i * (width + DAY_GAP), y);
      b.setOnClickListener(v -> toggleDay(day));
      dayButtons[day] = b;
      root.addView(b);
      paintDay(day);
    }
  }

  private void toggleDay(int day) {
    alarm.daysMask ^= 1 << day;
    paintDay(day);
  }

  private void paintDay(int day) {
    boolean on = (alarm.daysMask & (1 << day)) != 0;
    Button b = dayButtons[day];
    b.setTextColor(on ? 0xFF000000 : Ui.MUTED);
    b.setBackground(
        new GradientDrawable().setColor(on ? Ui.ACCENT : Ui.SURFACE).setCornerRadius(8));
  }

  private void save() {
    // Saving an alarm arms it: an alarm set and left off is almost always a
    // switch the user forgot, not one they meant.
    alarm.enabled = true;
    store.save(alarm.id);
    if (alarms != null) {
      alarms.reload();
    }
    Log.i(TAG, "saved alarm " + alarm.id + " " + alarm.time() + " " + alarm.repeatText());
    finish();
  }

  private void delete() {
    int id = alarm.id;
    store.delete(id);
    if (alarms != null) {
      alarms.reload();
    }
    Log.i(TAG, "deleted alarm " + id);
    finish();
  }

  private void cancel() {
    alarm.hour = wasHour;
    alarm.minute = wasMinute;
    alarm.daysMask = wasDays;
    alarm.enabled = wasEnabled;
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
