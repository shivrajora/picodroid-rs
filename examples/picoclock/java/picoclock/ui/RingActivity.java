// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import javax.inject.Inject;
import picoclock.Alarm;
import picoclock.AlarmStore;
import picoclock.Clock;
import picoclock.ClockApp;
import picodroid.content.Intent;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.FrameLayout;
import picodroid.widget.TextView;

/**
 * The alarm, going off. Two targets and nothing else on screen: snooze, and stop. Both are half the
 * panel tall, because the hand reaching for them is attached to someone who has just woken up.
 *
 * <p>{@link picoclock.AlarmService} owns the sound and the ring state; this screen only shows it
 * and sends the two verdicts back. That split is what lets the ring survive this screen — an alarm
 * that rings while the display is asleep still sounds, and this comes up when the panel wakes.
 */
public class RingActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  /** The alarm that is ringing, as put by whichever screen was up when it did. */
  public static final String EXTRA_ALARM_ID = "alarm";

  /**
   * Height of each of the two targets. Half the panel between them: the hand reaching for one is
   * attached to someone who has just woken up.
   */
  private static final int ACTION_HEIGHT = 128;

  /** Top of the clock face. Set so the caption clears the upper target. */
  private static final int FACE_TOP = 64;

  @Inject AlarmStore store;

  private SegmentClock face;
  private TextView caption;

  @Override
  public void onCreate() {
    super.onCreate();
    handlesRingItself = true;

    Intent intent = getIntent();
    int id = intent == null ? -1 : intent.getIntExtra(EXTRA_ALARM_ID, -1);

    FrameLayout root = Ui.screen();
    root.addView(Ui.header(this, "Alarm", null));

    face = new SegmentClock(root, SegmentClock.centredX(), FACE_TOP);

    String label = id < 0 ? "" : store.get(id).label;
    caption =
        Ui.centred(
            root,
            label.isEmpty() ? "Alarm" : label,
            FACE_TOP + SegmentClock.height() + 10,
            Ui.TEXT);

    int stopY = Ui.HEIGHT - ACTION_HEIGHT - Ui.MARGIN;
    int snoozeY = stopY - ACTION_HEIGHT - Ui.GAP;

    View snooze = action("Snooze " + AlarmStore.SNOOZE_MINUTES + " min", snoozeY, Ui.SURFACE_HIGH);
    snooze.setOnClickListener(v -> snooze());
    root.addView(snooze);

    View stop = action("Stop", stopY, Ui.ACCENT);
    stop.setOnClickListener(v -> dismiss());
    root.addView(stop);

    setContentView(root);
    redraw();
  }

  /**
   * BACK does not dismiss an alarm. A button pressed by a hand groping at a bedside table is not a
   * decision, and the two on-screen targets are unmissable.
   */
  @Override
  public void onBackPressed() {}

  @Override
  public void onTick() {
    redraw();
  }

  /** The service says the ring is over — by this screen's buttons, or from somewhere else. */
  @Override
  public void onAlarmStopped() {
    finish();
  }

  /** A second alarm coming due while this one rings just relabels the screen. */
  @Override
  public void onAlarmRing(Alarm alarm) {
    caption.setText(alarm.label.isEmpty() ? "Alarm" : alarm.label);
  }

  @Override
  protected void onAlarmsReady() {
    // Bound after the ring had already been dismissed elsewhere: do not sit
    // here showing an alarm that is not sounding.
    if (alarms != null && alarms.ringing() == null) {
      finish();
    }
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
    face.stop();
    super.onDestroy();
  }

  private View action(String text, int y, int fill) {
    View b = Ui.button(text, Ui.MARGIN, y, Ui.WIDTH - 2 * Ui.MARGIN, fill);
    b.setSize(Ui.WIDTH - 2 * Ui.MARGIN, ACTION_HEIGHT);
    return b;
  }

  private void snooze() {
    if (alarms != null) {
      alarms.snooze();
    }
    Log.i(TAG, "snooze tapped");
    finish();
  }

  private void dismiss() {
    if (alarms != null) {
      alarms.dismiss();
    }
    Log.i(TAG, "stop tapped");
    finish();
  }

  private void redraw() {
    long local = Clock.toLocal(System.currentTimeMillis(), store.offsetMinutes());
    // The colon blinks every half second; the digits only move once a minute,
    // which is all a ringing clock has to say.
    face.show(
        Clock.hourOf(local),
        Clock.minuteOf(local),
        Clock.msIntoDay(local) % Clock.MS_PER_SECOND < 500);
  }
}
