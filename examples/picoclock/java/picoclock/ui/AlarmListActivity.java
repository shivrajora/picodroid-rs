// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.Alarm;
import picoclock.AlarmStore;
import picoclock.ClockApp;
import picoclock.R;
import picodroid.content.Intent;
import picodroid.lifecycle.Lifecycle;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.Switch;
import picodroid.widget.TextView;

/**
 * Every alarm, one card each: its time, what it repeats on, and a switch that arms it without
 * opening anything. Tapping the card itself opens the editor; the last card adds a new alarm.
 *
 * <p>The cards are inflated from {@code res/layout/alarm_card.xml}, all of them whenever {@link
 * AlarmListViewModel#alarms} publishes, as an Android list would be built; the main loop allows a
 * screen build a slow first frame (docs/designs/ui-build-budget-2026-10.md). An edit made in the
 * editor publishes while this screen is stopped, and the list is rebuilt once when it comes back.
 */
public class AlarmListActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  private AlarmListViewModel model;
  private LinearLayout rows;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    model = viewModel(AlarmListViewModel.class);

    LinearLayout root = Ui.screen(this);
    Ui.appBar(this, root, true);

    rows = new LinearLayout(this);
    rows.setOrientation(LinearLayout.VERTICAL);
    ScrollView scroller = new ScrollView(this);
    // A ScrollView arrives with the theme's surface; this one is part of the page.
    scroller.setBackgroundColor(Ui.BACKGROUND);
    scroller.addView(
        rows,
        new ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    root.addView(
        scroller, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

    setContentView(root);
    // Rebuilt on a later turn of the main loop, not inside the delivery: a switch's own change
    // publishes, and the rebuild must not free the switch from under its click.
    model.alarms().observe(this, live -> rows.post(this::fill));
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

  // ── Rows ───────────────────────────────────────────────────────────────────

  /**
   * The list from scratch: one card per alarm, inflated from {@code alarm_card.xml}, then the "new
   * alarm" card.
   */
  private void fill() {
    if (getLifecycle().getCurrentState() == Lifecycle.State.DESTROYED) {
      return; // BACK landed between the post and this turn; the views are gone
    }
    Alarm[] live = model.alarms().getValue();
    rows.removeAllViews();
    Log.i(TAG, "alarms: " + live.length);
    LayoutInflater inflater = getLayoutInflater();
    for (Alarm alarm : live) {
      alarmCard(inflater, alarm);
    }
    newAlarmCard(inflater);
  }

  /** Inflates {@code layout} into the column as its last band and returns the band. */
  private View addCard(LayoutInflater inflater, int layout) {
    inflater.inflate(layout, rows, true);
    return rows.getChildAt(rows.getChildCount() - 1);
  }

  private void alarmCard(LayoutInflater inflater, Alarm alarm) {
    View band = addCard(inflater, R.layout.alarm_card);
    band.findViewById(R.id.hit).setOnClickListener(v -> edit(alarm.id));

    TextView time = band.findViewById(R.id.time);
    time.setText(alarm.time());
    time.setTextColor(alarm.enabled ? Ui.TEXT : Ui.MUTED);

    TextView sub = band.findViewById(R.id.sub);
    sub.setText(alarm.repeatText() + (alarm.label.isEmpty() ? "" : "  " + alarm.label));

    Switch armed = band.findViewById(R.id.armed);
    armed.setChecked(alarm.enabled);
    // The switch's change republishes the list, and the rebuild repaints the card.
    armed.setOnCheckedChangeListener((button, checked) -> model.setEnabled(alarm, checked));
  }

  private void newAlarmCard(LayoutInflater inflater) {
    View band = addCard(inflater, R.layout.new_alarm_card);
    band.findViewById(R.id.card).setOnClickListener(v -> newAlarm());
  }

  private void newAlarm() {
    int id = model.firstFree();
    if (id < 0) {
      Log.w(TAG, "no free alarm slot (" + AlarmStore.MAX_ALARMS + " in use)");
      return;
    }
    edit(id);
  }

  private void edit(int id) {
    startActivity(
        new Intent(this, AlarmEditActivity.class).putExtra(AlarmEditActivity.EXTRA_ID, id));
  }
}
