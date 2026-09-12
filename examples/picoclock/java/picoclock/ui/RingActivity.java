// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import javax.inject.Inject;
import picoclock.AlarmStore;
import picoclock.Clock;
import picoclock.ClockApp;
import picodroid.content.Intent;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.FrameLayout;

/**
 * The alarm, going off. Two targets and nothing else on screen: snooze, and stop. Both are half the
 * panel tall, because the hand reaching for them is attached to someone who has just woken up.
 *
 * <p>The framework starts this screen: an alarm set through {@code AlarmManager} comes back here
 * whatever the user was doing, this app included — if picoclock was not running, it is started for
 * the occasion and this lands on top of the clock face. The ring itself begins in {@link
 * picoclock.AlarmService#ring}, called once the service connection lands, and the service owns the
 * sound from then on; this screen only shows it and sends the two verdicts back.
 */
public class RingActivity extends BaseActivity {
  private static final String TAG = ClockApp.TAG;

  /** The alarm that is ringing, as the framework delivers it. */
  public static final String EXTRA_ALARM_ID = "alarm";

  /**
   * When the alarm was due, in whole epoch minutes ({@link picoclock.AlarmSchedule#epochMinute}).
   * The service checks it before sounding anything: a fire the wall clock jumped over is one for a
   * moment that never happened.
   */
  public static final String EXTRA_DUE_MINUTE = "due";

  /**
   * Height of each of the two targets. Half the panel between them: the hand reaching for one is
   * attached to someone who has just woken up.
   */
  private static final int ACTION_HEIGHT = 128;

  /** Top of the clock face. Set so the caption clears the upper target. */
  private static final int FACE_TOP = 64;

  @Inject AlarmStore store;

  private SegmentClock face;
  private int id = -1;
  private int dueMinute;

  /** Whether the ring has been started; the service connection can land more than once. */
  private boolean rung;

  @Override
  public void onCreate() {
    super.onCreate();

    Intent intent = getIntent();
    id = intent == null ? -1 : intent.getIntExtra(EXTRA_ALARM_ID, -1);
    dueMinute = intent == null ? 0 : intent.getIntExtra(EXTRA_DUE_MINUTE, 0);

    FrameLayout root = Ui.screen();
    root.addView(Ui.header(this, "Alarm", null));

    face = new SegmentClock(root, SegmentClock.centredX(), FACE_TOP);

    String label = store.exists(id) ? store.get(id).label : "";
    // One caption, set here and never changed: a second alarm due in the same
    // minute gets its own screen on top rather than editing this one's.
    Ui.centred(
        root, label.isEmpty() ? "Alarm" : label, FACE_TOP + SegmentClock.height() + 10, Ui.TEXT);

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

  /**
   * The service is up. This is where the ring starts — not in {@code onCreate}, because a bind is a
   * queued operation and the connection cannot have landed that early.
   */
  @Override
  protected void onAlarmsReady() {
    if (alarms == null || rung) {
      return;
    }
    rung = true;
    if (!alarms.ring(id, dueMinute)) {
      // Stale: the alarm was turned off, deleted, or the clock moved past it.
      // The service has re-armed what is left; nothing to show here.
      finish();
    }
  }

  // See BaseActivity: a lifecycle callback is only reached when the concrete
  // class declares it.
  @Override
  public void onResume() {
    super.onResume();
    // Two alarms due in the same minute stack two of these screens. When the
    // upper one is dealt with, the one underneath has nothing left to show.
    if (rung && alarms != null && alarms.ringing() == null) {
      finish();
    }
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
