// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import javax.inject.Inject;
import picodroid.app.AlarmManager;
import picodroid.app.Notification;
import picodroid.app.PendingIntent;
import picodroid.app.Service;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.TimeUnit;
import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.IntentFilter;
import picodroid.lifecycle.Observer;
import picodroid.os.IBinder;
import picodroid.util.Log;

/**
 * The part of the clock that keeps the alarms whether or not a screen is up: it hands them to the
 * framework, rings when one comes due, and drives the {@link Ringer} for as long as the ring
 * stands.
 *
 * <p>It does not decide when an alarm rings. {@link #plan} hands every armed alarm to the
 * framework's {@link AlarmManager}, which holds them outside this app's memory and starts {@link
 * picoclock.ui.RingActivity} when one comes due — after starting the app again, if the user has
 * gone elsewhere. That is the whole point: a timer of this app's own could only ever watch the
 * clock while the app was up, and pressing HOME tears the app down.
 *
 * <p>Everything here runs on the main thread. The ring's beat is a periodic task on {@link
 * Executors#mainScheduledExecutor} (what {@code Handler.postDelayed} is for on Android), armed when
 * a ring starts and cancelled when it ends, so nothing ticks between alarms and a Snooze tap
 * silences the ringer in the same call.
 */
public class AlarmService extends Service {
  private static final String TAG = ClockApp.TAG;
  private static final int NOTIFICATION_ID = 1;

  /** One beat of the ring: the {@link Ringer}'s pattern and the LED flash are counted in these. */
  static final int BEAT_MS = 250;

  /** What a bound screen hears. Every method arrives on the main thread. */
  public interface Listener {
    /** The ring ended, by snooze or dismissal. */
    void onAlarmStopped();
  }

  public static class LocalBinder implements IBinder {
    public AlarmService service;
  }

  private final LocalBinder binder = new LocalBinder();

  @Inject AlarmStore store;
  private AlarmManager alarms;
  private Ringer ringer;
  private final ScheduledExecutorService scheduler = Executors.mainScheduledExecutor();

  /** The ring's beat while an alarm is up, else null. */
  private ScheduledFuture<?> beating;

  /** The armed alarms, as {@link AlarmStore#alarms} last published them. */
  private Alarm[] armed = new Alarm[0];

  /** The one screen that is resumed, or null. */
  private Listener listener;

  /** The alarm currently ringing, or null. */
  private Alarm ringing;

  /**
   * When a snooze runs out, or 0. Deliberately not persisted: a snooze that outlived a restart
   * would be a promise this app cannot keep, since the framework's copy of the alarm is all that
   * survives and it carries no snooze count.
   */
  private long snoozeUntilUtcMs;

  /** The snoozed alarm, or null. */
  private Alarm snoozed;

  /** Beat counter for the ringer's pattern; reset at the start of each ring. */
  private int beat;

  /** An edit, a deletion or a switch on a screen: re-plan against the new list. */
  private final Observer<Alarm[]> onAlarmsChanged = this::replan;

  /**
   * The clock or the zone moved under the schedule: every armed instant was computed against the
   * old ones. Android's alarm clock listens for the same two actions.
   */
  private final BroadcastReceiver onClockChange =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          Log.i(TAG, "replanning: " + intent.getAction());
          reload();
        }
      };

  @Override
  public void onCreate() {
    binder.service = this;
    alarms = (AlarmManager) getSystemService(Context.ALARM_SERVICE);
    ringer = new Ringer();
    IntentFilter clock = new IntentFilter(Intent.ACTION_TIME_CHANGED);
    clock.addAction(Intent.ACTION_TIMEZONE_CHANGED);
    registerReceiver(onClockChange, clock);
    // observeForever delivers the current list at once, which is the first plan.
    store.alarms().observeForever(onAlarmsChanged);
    Log.i(TAG, "alarm service up, " + armed.length + " armed");
  }

  @Override
  public int onStartCommand(Intent intent, int flags, int startId) {
    startForeground(
        NOTIFICATION_ID,
        new Notification.Builder(this, "clock")
            .setContentTitle("Clock")
            .setContentText(statusText())
            .build());
    return START_STICKY;
  }

  @Override
  public IBinder onBind(Intent intent) {
    return binder;
  }

  @Override
  public boolean onUnbind(Intent intent) {
    listener = null;
    return true; // a later bind gets onRebind
  }

  @Override
  public void onRebind(Intent intent) {
    reload();
  }

  @Override
  public void onDestroy() {
    listener = null;
    store.alarms().removeObserver(onAlarmsChanged);
    unregisterReceiver(onClockChange);
    // A teardown mid-ring must not leave the sounder on with nothing left to turn it off.
    scheduler.shutdownNow();
    beating = null;
    ringing = null;
    ringer.release();
    stopForeground(true);
    Log.i(TAG, "alarm service down");
  }

  // ── The screens' side ──────────────────────────────────────────────────────

  /** The resumed screen, or null when none is. */
  public void setListener(Listener l) {
    listener = l;
  }

  /** Re-plan against the alarms as they are, after a clock or zone change. */
  private void reload() {
    replan(store.live());
  }

  /** {@link AlarmStore#alarms} published {@code live}: plan it with the framework. */
  private void replan(Alarm[] live) {
    armed = live;
    plan();
  }

  /**
   * Tell the framework about every alarm this app wants, and cancel the rest.
   *
   * <p>One operation per alarm slot rather than one for the soonest: two alarms set to the same
   * minute both have to ring, and a single operation could only carry one of them. The request code
   * is the alarm's id, so re-planning replaces an alarm's own entry and leaves the others alone.
   *
   * <p>An unset clock arms nothing. There is nothing to arm against — {@code currentTimeMillis}
   * counts from boot until somebody sets it — and the face says so.
   */
  private void plan() {
    boolean[] planned = new boolean[AlarmStore.MAX_ALARMS];
    long now = System.currentTimeMillis();
    if (Clock.isSet(now)) {
      for (Alarm a : armed) {
        long due = AlarmSchedule.nextFireUtcMs(a, now, store.offsetMinutes());
        if (due != AlarmSchedule.NEVER) {
          planned[a.id] = arm(a.id, due);
        }
      }
      // A snoozed alarm is handled outside that loop because a one-shot
      // disarms itself when it first rings: by now it is absent from
      // `armed`, and its snooze would be the one thing nobody re-armed.
      if (snoozed != null && snoozeUntilUtcMs != 0) {
        planned[snoozed.id] = arm(snoozed.id, snoozeUntilUtcMs);
      }
    }
    for (int id = 0; id < AlarmStore.MAX_ALARMS; id++) {
      if (!planned[id]) {
        alarms.cancel(operation(id, 0));
      }
    }
  }

  /** Hand one alarm to the framework; false if it refused, which is worth a line in the log. */
  private boolean arm(int id, long atUtcMs) {
    try {
      alarms.setExact(AlarmManager.RTC_WAKEUP, atUtcMs, operation(id, atUtcMs));
      return true;
    } catch (IllegalStateException | IllegalArgumentException e) {
      Log.w(TAG, "alarm " + id + " not armed: " + e.getMessage());
      return false;
    }
  }

  /**
   * The operation for alarm {@code id}: a broadcast to {@link AlarmReceiver}, Android's idiom,
   * which brings the ring screen up. Two of these are equal when their ids are, which is what lets
   * {@link #plan} replace and cancel by id; the due time rides along as an extra so {@link #ring}
   * can tell a fire it should honour from one the clock jumped over.
   */
  private PendingIntent operation(int id, long atUtcMs) {
    Intent intent =
        new Intent(this, AlarmReceiver.class)
            .putExtra(picoclock.ui.RingActivity.EXTRA_ALARM_ID, id)
            .putExtra(
                picoclock.ui.RingActivity.EXTRA_DUE_MINUTE, AlarmSchedule.epochMinute(atUtcMs));
    return PendingIntent.getBroadcast(
        this, id, intent, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
  }

  /**
   * Start ringing alarm {@code id}, which the framework has just brought a screen up for. Returns
   * false when the fire is stale — the clock moved, or the alarm was turned off between the arming
   * and now — in which case the alarms have been re-planned and the caller should take its screen
   * away again.
   */
  public boolean ring(int id, int dueMinute) {
    long now = System.currentTimeMillis();
    Alarm a = store.exists(id) ? store.get(id) : null;
    boolean fromSnooze = a != null && a == snoozed;
    snoozed = null;
    snoozeUntilUtcMs = 0;

    if (a == null
        || !(a.enabled || fromSnooze)
        || !AlarmSchedule.shouldRingMinute(dueMinute, now)) {
      Log.i(TAG, "stale fire for alarm " + id + ", re-armed");
      reload();
      return false;
    }

    Log.i(TAG, "ring " + a.time() + (a.label.isEmpty() ? "" : " " + a.label));
    // A one-shot disarms itself once it has rung, as on Android. A snooze is
    // the same alarm ringing again, so it does not disarm anything twice. The
    // save republishes the list, which re-plans.
    if (!a.repeats() && !fromSnooze) {
      a.enabled = false;
      store.save(id);
    } else {
      reload();
    }

    // The pattern starts over; a ring this one supersedes hands over its beat task.
    ringing = a;
    beat = 0;
    if (beating == null) {
      beating = scheduler.scheduleAtFixedRate(this::onBeat, 0, BEAT_MS, TimeUnit.MILLISECONDS);
    }
    return true;
  }

  /** The alarm currently ringing, or null. */
  public Alarm ringing() {
    return ringing;
  }

  /** Silence the ring and re-arm it {@link AlarmStore#SNOOZE_MINUTES} out. */
  public void snooze() {
    Alarm up = ringing;
    if (up == null) {
      return;
    }
    snoozed = up;
    snoozeUntilUtcMs = System.currentTimeMillis() + AlarmStore.SNOOZE_MINUTES * Clock.MS_PER_MINUTE;
    Log.i(TAG, "snoozed " + up.time() + " for " + AlarmStore.SNOOZE_MINUTES + " min");
    stopRing();
    plan();
  }

  /** Silence the ring for good; a repeating alarm still rings at its next occurrence. */
  public void dismiss() {
    Alarm up = ringing;
    if (up == null) {
      return;
    }
    Log.i(TAG, "dismissed " + up.time());
    stopRing();
  }

  /** When the next alarm rings, or {@link AlarmSchedule#NEVER}. Includes a pending snooze. */
  public long nextFireUtcMs() {
    long soonest =
        AlarmSchedule.nextFireUtcMs(armed, System.currentTimeMillis(), store.offsetMinutes());
    long snooze = snoozeUntilUtcMs;
    if (snooze != 0 && snooze < soonest) {
      return snooze;
    }
    return soonest;
  }

  /** The alarm {@link #nextFireUtcMs} belongs to, or null. */
  public Alarm nextAlarm() {
    long now = System.currentTimeMillis();
    long snooze = snoozeUntilUtcMs;
    if (snooze != 0 && snooze < AlarmSchedule.nextFireUtcMs(armed, now, store.offsetMinutes())) {
      return snoozed;
    }
    return AlarmSchedule.nextAlarm(armed, now, store.offsetMinutes());
  }

  // ── The ring ───────────────────────────────────────────────────────────────

  /** One beat of the ring, every {@link #BEAT_MS} while an alarm is up. */
  private void onBeat() {
    if (ringing != null) {
      ringer.beat(beat++);
    }
  }

  /**
   * End the ring: cancel the beat, silence the ringer, and tell the resumed screen. The screen
   * hears it on a later turn of the main loop, not inside this call: the ring screen that tapped
   * Snooze finishes itself, and must not be finished a second time from under its own click.
   */
  private void stopRing() {
    ringing = null;
    if (beating != null) {
      beating.cancel(false);
      beating = null;
    }
    ringer.off();
    getMainExecutor()
        .execute(
            () -> {
              Listener l = listener;
              if (l != null) {
                l.onAlarmStopped();
              }
            });
  }

  private String statusText() {
    long at = nextFireUtcMs();
    if (at == AlarmSchedule.NEVER) {
      return "No alarm set";
    }
    long local = Clock.toLocal(at, store.offsetMinutes());
    return "Next alarm " + Clock.hm(Clock.hourOf(local), Clock.minuteOf(local));
  }
}
