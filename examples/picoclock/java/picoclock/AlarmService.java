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
 *
 * <h2>Which thread owns what</h2>
 *
 * Two threads meet here and the split between them is deliberate, because getting it wrong leaves a
 * board that buzzes with nothing on screen to stop it.
 *
 * <ul>
 *   <li><b>The ticker owns the {@link Buzzer} and the ring.</b> No other thread calls it, not even
 *       to silence it. A screen tapping Snooze records a {@linkplain #verdict verdict} and the
 *       ticker acts on it within a tick. The obvious shortcut — let the tap call {@code
 *       buzzer.off()} directly — loses to the interleaving where the ticker has already decided an
 *       alarm is ringing and sounds the next beat after the silence, with the ring screen gone.
 *   <li><b>The ticker owns the snooze and every due time.</b> The screens only read them.
 *   <li><b>The main thread owns the alarms themselves</b> and hands the ticker a whole new {@link
 *       Schedule} at once. The ticker never mutates an {@link Alarm}; a one-shot disarming itself
 *       does so through a Runnable posted back to the main thread.
 * </ul>
 */
public class AlarmService extends Service {
  private static final String TAG = ClockApp.TAG;
  private static final int NOTIFICATION_ID = 1;

  /**
   * How often {@link #tick} runs. Fast enough that a ring lands within a quarter second of its
   * minute and that the buzzer's two-beep pattern has a beat, slow enough to be invisible.
   */
  private static final int TICK_MS = 250;

  /**
   * How long the ticker sleeps between checks that it should still be running. A tick is a whole
   * number of these. Sleeping the full {@link #TICK_MS} instead would mean {@code onDestroy}
   * waiting up to a quarter second for the thread to notice, which is long enough for the
   * slow-handler watchdog to call it a stall.
   */
  private static final int STOP_POLL_MS = 25;

  private static final int POLLS_PER_TICK = TICK_MS / STOP_POLL_MS;

  /** How long {@code onDestroy} waits for the ticker to release the buzzer. */
  private static final long TICKER_JOIN_MS = 250;

  /** What a screen has decided about the ring that is up. Read and cleared by the ticker. */
  private static final int VERDICT_NONE = 0;

  private static final int VERDICT_SNOOZE = 1;
  private static final int VERDICT_DISMISS = 2;

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

  /**
   * The armed alarms and when each next fires, as one object so the pair can be replaced with a
   * single assignment. They used to be two fields, and a {@link #reload} landing between the two
   * writes left the ticker walking a longer alarm array against a shorter array of due times.
   */
  private static final class Schedule {
    final Alarm[] armed;

    /** Parallel to {@link #armed}. Mutated only by the ticker, as alarms fire and re-arm. */
    final long[] dueUtcMs;

    Schedule(Alarm[] armed, long[] dueUtcMs) {
      this.armed = armed;
      this.dueUtcMs = dueUtcMs;
    }
  }

  private static final Schedule EMPTY = new Schedule(new Alarm[0], new long[0]);

  private final LocalBinder binder = new LocalBinder();

  @Inject AlarmStore store;
  private Buzzer buzzer;
  private Thread ticker;
  private volatile boolean running;

  /** Replaced whole by the main thread, read once per tick by the ticker. */
  private volatile Schedule schedule = EMPTY;

  /**
   * The one screen that is resumed, or null. Written from the main thread and read by the ticker,
   * hence volatile — the ticker only ever hands it to {@code mainExecutor}, never calls it.
   */
  private volatile Listener listener;

  /** The alarm currently ringing, or null. Written by the ticker. */
  private volatile Alarm ringing;

  /** A screen's decision about the ring, waiting for the ticker to act on it. */
  private volatile int verdict = VERDICT_NONE;

  /** When a snooze runs out, or 0. Written by the ticker, read by the screens. */
  private volatile long snoozeUntilUtcMs;

  /** The snoozed alarm. Written by the ticker, read by the screens. */
  private volatile Alarm snoozed;

  /** Beat counter for the buzzer pattern; reset at the start of each ring. Ticker only. */
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
    Log.i(TAG, "alarm service up, " + schedule.armed.length + " armed");
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
    // The ticker silences and releases the buzzer as it exits. Waiting for it
    // is what keeps this thread off those pins; the alternative is a teardown
    // mid-ring that leaves the sounder on with nothing left to turn it off.
    joinTicker();
    stopForeground(true);
    Log.i(TAG, "alarm service down");
  }

  private void joinTicker() {
    Thread t = ticker;
    ticker = null;
    if (t == null) {
      return;
    }
    try {
      t.join(TICKER_JOIN_MS);
    } catch (InterruptedException e) {
      Thread.currentThread().interrupt();
    }
    if (t.isAlive()) {
      // Wedged. Reaching for the buzzer from here would be the very race this
      // whole arrangement exists to avoid, so say so instead.
      Log.w(TAG, "ticker did not stop in " + TICKER_JOIN_MS + " ms; buzzer not released");
    }
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

  /** Re-read the alarms after an edit and recompute every due time. Main thread only. */
  public void reload() {
    Alarm[] armed = store.live();
    long[] due = new long[armed.length];
    long now = System.currentTimeMillis();
    for (int i = 0; i < armed.length; i++) {
      due[i] = AlarmSchedule.nextFireUtcMs(armed[i], now, store.offsetMinutes());
    }
    schedule = new Schedule(armed, due);
  }

  /** The alarm currently ringing, or null. */
  public Alarm ringing() {
    return ringing;
  }

  /**
   * Silence the ring and re-arm it {@link AlarmStore#SNOOZE_MINUTES} out. Takes effect on the next
   * tick; the screen calling this finishes itself and does not wait.
   */
  public void snooze() {
    verdict = VERDICT_SNOOZE;
  }

  /** Silence the ring for good; a repeating alarm still rings at its next occurrence. */
  public void dismiss() {
    verdict = VERDICT_DISMISS;
  }

  /** When the next alarm rings, or {@link AlarmSchedule#NEVER}. Includes a pending snooze. */
  public long nextFireUtcMs() {
    long soonest =
        AlarmSchedule.nextFireUtcMs(
            schedule.armed, System.currentTimeMillis(), store.offsetMinutes());
    long snooze = snoozeUntilUtcMs;
    if (snooze != 0 && snooze < soonest) {
      return snooze;
    }
    return soonest;
  }

  /** The alarm {@link #nextFireUtcMs} belongs to, or null. */
  public Alarm nextAlarm() {
    Schedule s = schedule;
    long now = System.currentTimeMillis();
    long snooze = snoozeUntilUtcMs;
    if (snooze != 0 && snooze < AlarmSchedule.nextFireUtcMs(s.armed, now, store.offsetMinutes())) {
      return snoozed;
    }
    return AlarmSchedule.nextAlarm(s.armed, now, store.offsetMinutes());
  }

  // ── The ticker ─────────────────────────────────────────────────────────────

  private void tickLoop() {
    int polls = 0;
    while (running) {
      SystemClock.sleep(STOP_POLL_MS);
      if (!running) {
        break;
      }
      if (++polls < POLLS_PER_TICK) {
        continue;
      }
      polls = 0;
      try {
        tick(System.currentTimeMillis());
      } catch (RuntimeException e) {
        // A fault here would leave the board with no alarms at all, silently.
        Log.w(TAG, "tick failed: " + e);
      }
    }
    // The buzzer belongs to this thread for its whole life, this end included.
    buzzer.off();
    buzzer.close();
  }

  private void tick(long now) {
    Executors.mainExecutor().execute(tickNotify);
    applyVerdict(now);
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
    Schedule s = schedule;
    for (int i = 0; i < s.armed.length; i++) {
      if (!AlarmSchedule.shouldRing(s.dueUtcMs[i], now)) {
        if (s.dueUtcMs[i] != AlarmSchedule.NEVER && now > s.dueUtcMs[i]) {
          // Missed: the wall clock jumped over it. Re-arm, do not ring.
          s.dueUtcMs[i] = AlarmSchedule.nextFireUtcMs(s.armed[i], now, store.offsetMinutes());
        }
        continue;
      }
      Alarm due = s.armed[i];
      if (due.repeats()) {
        s.dueUtcMs[i] = AlarmSchedule.nextFireUtcMs(due, dueAt(s, i, now), store.offsetMinutes());
      } else {
        // A one-shot disarms itself once it has rung, as on Android. Clearing
        // the due time here is what stops it ringing again; the alarm object
        // itself is the main thread's, so that edit is posted there.
        s.dueUtcMs[i] = AlarmSchedule.NEVER;
        Executors.mainExecutor()
            .execute(
                () -> {
                  due.enabled = false;
                  store.save(due.id);
                });
      }
      startRing(due);
      return;
    }
  }

  /** Apply a screen's Snooze or Stop. Ticker only — this is where the buzzer goes quiet. */
  private void applyVerdict(long now) {
    int v = verdict;
    if (v == VERDICT_NONE) {
      return;
    }
    verdict = VERDICT_NONE;
    Alarm up = ringing;
    if (up == null) {
      return; // the ring ended on its own between the tap and this tick
    }
    if (v == VERDICT_SNOOZE) {
      snoozed = up;
      snoozeUntilUtcMs = now + AlarmStore.SNOOZE_MINUTES * Clock.MS_PER_MINUTE;
      Log.i(TAG, "snoozed " + up.time() + " for " + AlarmStore.SNOOZE_MINUTES + " min");
    } else {
      snoozed = null;
      snoozeUntilUtcMs = 0;
      Log.i(TAG, "dismissed " + up.time());
    }
    stopRing();
  }

  /** The instant fire {@code i} was due, so the next one is computed from the schedule, not now. */
  private static long dueAt(Schedule s, int i, long now) {
    return s.dueUtcMs[i] == AlarmSchedule.NEVER ? now : s.dueUtcMs[i];
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

  /** Ticker only: {@link #applyVerdict} and the tick loop's exit are the only callers. */
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
