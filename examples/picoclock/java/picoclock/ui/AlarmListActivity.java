// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import javax.inject.Inject;
import picoclock.Alarm;
import picoclock.AlarmStore;
import picoclock.ClockApp;
import picodroid.concurrent.Executors;
import picodroid.content.Intent;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.util.Log;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.Switch;
import picodroid.widget.TextView;

/**
 * Every alarm, one card each: its time, what it repeats on, and a switch that arms it without
 * opening anything. Tapping the card itself opens the editor; the last card adds a new alarm.
 *
 * <p>The cards are added one per UI tick rather than all inside {@code onCreate}. Each is four LVGL
 * objects, about 25 ms of work on this chip, so a full list of eight built in one go would hold the
 * UI thread for a fifth of a second and trip the slow-handler watchdog — the same reason the
 * launcher and the settings app build their rows this way.
 */
public class AlarmListActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  private static final int ROW_HEIGHT = 72;
  private static final int ROW_GAP = 8;

  /** Width of the card's tappable area; the switch takes the rest. */
  private static final int TAP_AREA_WIDTH = 240;

  private static final int SWITCH_WIDTH = 52;
  private static final int SWITCH_HEIGHT = 28;

  @Inject AlarmStore store;

  private LinearLayout rows;

  /** The alarm ids on screen, in row order; held so their listeners stay reachable. */
  private int[] shown = new int[0];

  private boolean stopped;

  /**
   * Bumped by every {@link #fill}. The chain that adds the cards carries the value it started with
   * and stops as soon as it no longer matches, so a rebuild cannot interleave with the chain it
   * replaced — which is what put two "New alarm" cards on the screen when {@code onCreate} and
   * {@code onResume} each started one.
   */
  private int generation;

  @Override
  public void onCreate() {
    super.onCreate();
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
    // again on every return from the editor. Doing it here as well would be
    // a second chain racing the first.
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

  @Override
  public void onDestroy() {
    stopped = true;
    super.onDestroy();
  }

  // ── Rows ───────────────────────────────────────────────────────────────────

  private void fill() {
    generation++;
    rows.removeAllViews();
    Alarm[] live = store.live();
    shown = new int[live.length];
    for (int i = 0; i < live.length; i++) {
      shown[i] = live[i].id;
    }
    // +1 for the "new alarm" card; the column is sized for it up front so the
    // ScrollView does not resize under the finger as rows land.
    rows.setSize(Ui.WIDTH, height(shown.length + 1));
    Log.i(TAG, "alarms: " + shown.length);
    final int mine = generation;
    Executors.mainExecutor().execute(() -> addNext(0, mine));
  }

  private void addNext(int i, int mine) {
    if (stopped || mine != generation) {
      return;
    }
    if (i < shown.length) {
      alarmCard(store.get(shown[i]));
    } else if (i == shown.length) {
      newAlarmCard();
    } else {
      return;
    }
    Executors.mainExecutor().execute(() -> addNext(i + 1, mine));
  }

  /**
   * Appends an empty card to the column and returns it for the caller to fill. The column's child
   * is a full-width band; the card is inset within it, which is what puts the gap between cards
   * without a spacing rule that the ScrollView would have to know about.
   */
  private FrameLayout addCard() {
    FrameLayout band = Ui.group(0, 0, Ui.WIDTH, ROW_HEIGHT + ROW_GAP);

    FrameLayout card = new FrameLayout();
    card.setSize(Ui.WIDTH - 2 * Ui.MARGIN, ROW_HEIGHT);
    card.setPosition(Ui.MARGIN, ROW_GAP);
    card.setPadding(0, 0, 0, 0);
    card.setBackground(new GradientDrawable().setColor(Ui.SURFACE).setCornerRadius(10));
    band.addView(card);
    rows.addView(band);
    return card;
  }

  private void alarmCard(Alarm alarm) {
    FrameLayout card = addCard();

    FrameLayout hit = new FrameLayout();
    hit.setSize(TAP_AREA_WIDTH, ROW_HEIGHT);
    hit.setPosition(0, 0);
    hit.setPadding(0, 0, 0, 0);
    hit.setBackground(new GradientDrawable().setColor(Ui.SURFACE).setCornerRadius(10));
    hit.setOnClickListener(v -> edit(alarm.id));
    card.addView(hit);

    TextView time = Ui.label(alarm.time(), Ui.MARGIN, 12, alarm.enabled ? Ui.TEXT : Ui.MUTED);
    hit.addView(time);

    String detail = alarm.repeatText() + (alarm.label.isEmpty() ? "" : "  " + alarm.label);
    TextView sub = Ui.label(detail, Ui.MARGIN, 42, Ui.MUTED);
    sub.setSize(TAP_AREA_WIDTH - 2 * Ui.MARGIN, 20);
    hit.addView(sub);

    Switch armed = new Switch();
    armed.setSize(SWITCH_WIDTH, SWITCH_HEIGHT);
    armed.setPosition(
        Ui.WIDTH - 2 * Ui.MARGIN - SWITCH_WIDTH - Ui.MARGIN, (ROW_HEIGHT - SWITCH_HEIGHT) / 2);
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
    card.addView(armed);
  }

  private void newAlarmCard() {
    FrameLayout card = addCard();
    card.setBackground(new GradientDrawable().setColor(Ui.SURFACE_HIGH).setCornerRadius(10));
    card.addView(Ui.label("+  New alarm", Ui.MARGIN, ROW_HEIGHT / 2 - 10, Ui.ACCENT));
    card.setOnClickListener(v -> newAlarm());
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
