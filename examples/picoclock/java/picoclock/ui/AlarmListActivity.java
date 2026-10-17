// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import javax.inject.Inject;
import picoclock.Alarm;
import picoclock.AlarmStore;
import picoclock.ClockApp;
import picoclock.R;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.Switch;
import picodroid.widget.TextView;

/**
 * Every alarm, one card each: its time, what it repeats on, and a switch that arms it without
 * opening anything. Tapping the card itself opens the editor; the last card adds a new alarm.
 *
 * <p>The cards are inflated from {@code res/layout/alarm_card.xml}, all of them in {@link
 * #onResume}, as an Android list would be built; the main loop allows a screen build a slow first
 * frame (docs/designs/ui-build-budget-2026-10.md).
 */
public class AlarmListActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  /** A card's height and the gap above it: what {@code alarm_card.xml}'s band is tall. */
  private static final int ROW_HEIGHT = 72;

  private static final int ROW_GAP = 8;

  @Inject AlarmStore store;

  private LinearLayout rows;

  /** The alarm ids on screen, in row order; held so their listeners stay reachable. */
  private int[] shown = new int[0];

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    FrameLayout root = Ui.screen();
    root.addView(Ui.header(this, "Alarms", v -> finish()));

    rows = Ui.list();
    rows.setSize(Ui.WIDTH, Ui.HEIGHT);
    rows.setBackground(Ui.invisible());

    ScrollView scroller = new ScrollView();
    scroller.setSize(Ui.WIDTH, Ui.HEIGHT - Ui.HEADER_HEIGHT);
    scroller.setPosition(0, Ui.HEADER_HEIGHT);
    scroller.setPadding(0, 0, 0, 0);
    scroller.setBackground(Ui.invisible());
    scroller.addView(rows);
    root.addView(scroller);

    setContentView(root);
    // The rows are filled in from onResume, which runs once on the way in and
    // again on every return from the editor.
  }

  @Override
  public void onResume() {
    super.onResume();
    // An edit or a deletion happened behind us: rebuild rather than reconcile.
    fill();
  }

  // See BaseActivity: a lifecycle callback is only reached when the concrete
  // class declares it.
  @Override
  public void onPause() {
    super.onPause();
  }

  // ── Rows ───────────────────────────────────────────────────────────────────

  /**
   * The list from scratch: one card per alarm, inflated from {@code alarm_card.xml}, then the "new
   * alarm" card.
   */
  private void fill() {
    rows.removeAllViews();
    Alarm[] live = store.live();
    shown = new int[live.length];
    for (int i = 0; i < live.length; i++) {
      shown[i] = live[i].id;
    }
    // +1 for the "new alarm" card.
    rows.setSize(Ui.WIDTH, height(shown.length + 1));
    Log.i(TAG, "alarms: " + shown.length);
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
    armed.setOnCheckedChangeListener(
        (button, checked) -> {
          store.setEnabled(alarm.id, checked);
          time.setTextColor(checked ? Ui.TEXT : Ui.MUTED);
          if (alarms != null) {
            alarms.reload();
          }
          Log.i(TAG, "alarm " + alarm.id + " " + alarm.time() + (checked ? " on" : " off"));
        });
  }

  private void newAlarmCard(LayoutInflater inflater) {
    View band = addCard(inflater, R.layout.new_alarm_card);
    band.findViewById(R.id.card).setOnClickListener(v -> newAlarm());
  }

  private void newAlarm() {
    int id = store.firstFree();
    if (id < 0) {
      Log.w(TAG, "no free alarm slot (" + AlarmStore.MAX_ALARMS + " in use)");
      return;
    }
    edit(id);
  }

  private void edit(int id) {
    startActivity(new Intent(AlarmEditActivity.class).putExtra(AlarmEditActivity.EXTRA_ID, id));
  }

  private static int height(int cards) {
    int content = cards * (ROW_HEIGHT + ROW_GAP) + ROW_GAP;
    int screen = Ui.HEIGHT - Ui.HEADER_HEIGHT;
    return content > screen ? content : screen;
  }
}
