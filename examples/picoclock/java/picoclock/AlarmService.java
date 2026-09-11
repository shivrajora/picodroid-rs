// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import javax.inject.Inject;
import picodroid.app.Notification;
import picodroid.app.Service;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.Intent;
import picodroid.os.IBinder;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * The part of the clock that keeps time whether or not a screen is up: it watches the wall clock,
 * rings when an alarm comes due, and drives the {@link Buzzer} for as long as the ring stands.
 *
 * <p>A foreground Service with one thread of its own. There is no {@code Handler}, {@code
 * postDelayed} or {@code ScheduledExecutorService} on this platform, so periodic work is a thread
 * that sleeps and posts — every callback below reaches its listener through {@code
 * Executors.mainExecutor()}, so a screen never touches a widget from the ticker.
 *
 * <p>Activities bind to it rather than start it: {@link picoclock.ui.BaseActivity} does the binding
 * and routes a ring to the screen. The Service is also started, so it outlives the last unbind and
 * an alarm still rings while the user is on another screen of the app.
 */
public class AlarmService extends Service {
  private static final String TAG = ClockApp.TAG;
  private static final int NOTIFICATION_ID = 1;

  /**
   * Ticker period. Fast enough that a ring lands within a quarter second of its minute and that the
   * buzzer's two-beep pattern has a beat, slow enough to be invisible on a 150 MHz core.
   */
  private static final int TICK_MS = 250;

  /** What a bound screen hears. Every method arrives on the main thread. */
  public interface Listener {
    /** An alarm has come due, or a snooze has run out. */
    void onAlarmRing(Alarm alarm);

    /** The ring ended, by snooze or dismissal. */
    void onAlarmStopped();

    /**
     * One service tick, {@link #TICK_MS} apart. This is the app's only heartbeat: the clock face
     * redraws off it rather than owning a thread or a timer of its own, neither of which this
     * platform offers for free.
     */
    void onTick();
  }

  public static class LocalBinder implements IBinder {
    public AlarmService service;
  }

  private final LocalBinder binder = new LocalBinder();

  @Inject AlarmStore store;
  private Buzzer buzzer;
  private Thread ticker;
  private volatile boolean running;

  /**
   * The one screen that is resumed, or null. Written from the main thread and read by the ticker,
   * hence volatile — the ticker only ever hands it to {@code mainExecutor}, never calls it.
   */
  private volatile Listener listener;

  /** Precomputed next fire per live alarm, parallel to {@link #armed}. */
  private long[] dueUtcMs = new long[0];

  private Alarm[] armed = new Alarm[0];

  /** The alarm currently ringing, or null. */
  private volatile Alarm ringing;

  /** When a snooze runs out, or 0. Survives the ring being dismissed off-screen. */
  private long snoozeUntilUtcMs;

  private Alarm snoozed;

  /** Beat counter for the buzzer pattern; reset at the start of each ring. */
  private int beat;

  /**
   * The tick notification, allocated once and posted every tick. A fresh lambda here would be four
   * short-lived objects a second for as long as the board is up — the GC would cope, but paying
   * nothing is better than paying that.
   */
  @SuppressWarnings("UnnecessaryLambda") // a method reference would allocate per tick; see above
  private final Runnable tickNotify =
      () -> {
        Listener l = listener;
        if (l != null) {
          l.onTick();
        }
      };

  @Override
  public void onCreate() {
    binder.service = this;
    buzzer = new Buzzer();
    reload();
    running = true;
    ticker = new Thread(this::tickLoop);
    ticker.start();
    Log.i(TAG, "alarm service up, " + armed.length + " armed");
  }

  @Override
  public int onStartCommand(Intent intent, int flags, int startId) {
    startForeground(
        NOTIFICATION_ID,
        new Notification.Builder().setContentTitle("Clock").setContentText(statusText()).build());
    return START_STICKY;
  }

  @Override
  public IBinder onBind(Intent intent) {
    return binder;
  }

  @Override
  public boolean onUnbind(Intent intent) {
    listener = null;
    return true; // a later bind gets onRebind; the ticker keeps running meanwhile
  }

  @Override
  public void onRebind(Intent intent) {
    reload();
  }

  @Override
  public void onDestroy() {
    running = false;
    listener = null;
    buzzer.close();
    stopForeground(true);
    Log.i(TAG, "alarm service down");
  }

  // ── The screens' side ──────────────────────────────────────────────────────

  /** The resumed screen, or null when none is. Main thread only. */
  public void setListener(Listener l) {
    listener = l;
    Alarm up = ringing;
    if (l != null && up != null) {
      // A screen that binds mid-ring learns about it rather than showing a clock
      // over a sounding buzzer.
      l.onAlarmRing(up);
    }
  }

  /** Re-read the alarms after an edit and recompute every due time. */
  public void reload() {
    armed = store.live();
    dueUtcMs = new long[armed.length];
    long now = System.currentTimeMillis();
    for (int i = 0; i < armed.length; i++) {
      dueUtcMs[i] = AlarmSchedule.nextFireUtcMs(armed[i], now, store.offsetMinutes());
    }
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
    stopRing();
    Log.i(TAG, "snoozed " + up.time() + " for " + AlarmStore.SNOOZE_MINUTES + " min");
  }

  /** Silence the ring for good; a repeating alarm still rings at its next occurrence. */
  public void dismiss() {
    Alarm up = ringing;
    if (up == null) {
      return;
    }
    snoozed = null;
    snoozeUntilUtcMs = 0;
    stopRing();
    Log.i(TAG, "dismissed " + up.time());
  }

  /** When the next alarm rings, or {@link AlarmSchedule#NEVER}. Includes a pending snooze. */
  public long nextFireUtcMs() {
    long soonest =
        AlarmSchedule.nextFireUtcMs(armed, System.currentTimeMillis(), store.offsetMinutes());
    if (snoozeUntilUtcMs != 0 && snoozeUntilUtcMs < soonest) {
      return snoozeUntilUtcMs;
    }
    return soonest;
  }

  /** The alarm {@link #nextFireUtcMs} belongs to, or null. */
  public Alarm nextAlarm() {
    if (snoozeUntilUtcMs != 0 && snoozeUntilUtcMs <= nextFireUtcMs()) {
      return snoozed;
    }
    return AlarmSchedule.nextAlarm(armed, System.currentTimeMillis(), store.offsetMinutes());
  }

  // ── The ticker ─────────────────────────────────────────────────────────────

  private void tickLoop() {
    while (running) {
      SystemClock.sleep(TICK_MS);
      if (!running) {
        return;
      }
      try {
        tick(System.currentTimeMillis());
      } catch (RuntimeException e) {
        // A fault here would leave the board with no alarms at all, silently.
        Log.w(TAG, "tick failed: " + e);
      }
    }
  }

  private void tick(long now) {
    Executors.mainExecutor().execute(tickNotify);
    if (ringing != null) {
      buzzer.beat(beat++);
      return;
    }
    if (!Clock.isSet(now)) {
      return; // nothing to compare against until the clock is set
    }
    if (snoozeUntilUtcMs != 0 && AlarmSchedule.shouldRing(snoozeUntilUtcMs, now)) {
      Alarm again = snoozed;
      snoozeUntilUtcMs = 0;
      snoozed = null;
      startRing(again);
      return;
    }
    for (int i = 0; i < armed.length; i++) {
      if (!AlarmSchedule.shouldRing(dueUtcMs[i], now)) {
        if (dueUtcMs[i] != AlarmSchedule.NEVER && now > dueUtcMs[i]) {
          // Missed: the wall clock jumped over it. Re-arm, do not ring.
          dueUtcMs[i] = AlarmSchedule.nextFireUtcMs(armed[i], now, store.offsetMinutes());
        }
        continue;
      }
      Alarm due = armed[i];
      if (due.repeats()) {
        dueUtcMs[i] = AlarmSchedule.nextFireUtcMs(due, due(i, now), store.offsetMinutes());
      } else {
        // A one-shot disarms itself once it has rung, as on Android.
        due.enabled = false;
        dueUtcMs[i] = AlarmSchedule.NEVER;
        Executors.mainExecutor().execute(() -> store.save(due.id));
      }
      startRing(due);
      return;
    }
  }

  /** The instant fire {@code i} was due, so the next one is computed from the schedule, not now. */
  private long due(int i, long now) {
    return dueUtcMs[i] == AlarmSchedule.NEVER ? now : dueUtcMs[i];
  }

  private void startRing(Alarm a) {
    ringing = a;
    beat = 0;
    Log.i(TAG, "ring " + a.time() + (a.label.isEmpty() ? "" : " " + a.label));
    Executors.mainExecutor()
        .execute(
            () -> {
              Listener l = listener;
              if (l != null) {
                l.onAlarmRing(a);
              }
            });
  }

  private void stopRing() {
    ringing = null;
    buzzer.off();
    Executors.mainExecutor()
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
