// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import javax.inject.Inject;
import picodroid.app.AlarmManager;
import picodroid.app.Notification;
import picodroid.app.PendingIntent;
import picodroid.app.Service;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.Context;
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
 * <p>It does not decide when an alarm rings. {@link #plan} hands every armed alarm to the
 * framework's {@link AlarmManager}, which holds them outside this app's memory and starts {@link
 * picoclock.ui.RingActivity} when one comes due — after starting the app again, if the user has
 * gone elsewhere. That is the whole point: a thread of this app's own could only ever watch the
 * clock while the app was up, and pressing HOME tears the app down.
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
 *   <li><b>The main thread starts a ring</b> — {@link #ring}, called by the screen the framework
 *       brought up — and owns the alarms, the snooze and the {@link Schedule}. The ticker reads
 *       {@link #ringing} to know whether to sound a beat, and never mutates an {@link Alarm}.
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

    /** Parallel to {@link #armed}: when each next fires. Written once, at {@link #reload}. */
    final long[] dueUtcMs;

    Schedule(Alarm[] armed, long[] dueUtcMs) {
      this.armed = armed;
      this.dueUtcMs = dueUtcMs;
    }
  }

  private static final Schedule EMPTY = new Schedule(new Alarm[0], new long[0]);

  private final LocalBinder binder = new LocalBinder();

  @Inject AlarmStore store;
  private AlarmManager alarms;
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

  /** The alarm currently ringing, or null. Written by the main thread, read by the ticker. */
  private volatile Alarm ringing;

  /** A screen's decision about the ring, waiting for the ticker to act on it. */
  private volatile int verdict = VERDICT_NONE;

  /**
   * When a snooze runs out, or 0. Main thread only, and deliberately not persisted: a snooze that
   * outlived a restart would be a promise this app cannot keep, since the framework's copy of the
   * alarm is all that survives and it carries no snooze count.
   */
  private long snoozeUntilUtcMs;

  /** The snoozed alarm, or null. Main thread only. */
  private Alarm snoozed;

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
    alarms = (AlarmManager) getSystemService(Context.ALARM_SERVICE);
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
  }

  /**
   * Re-read the alarms after an edit, recompute every due time, and hand the lot to the framework.
   * Main thread only. Every screen that changes an alarm ends by calling this.
   */
  public void reload() {
    Alarm[] armed = store.live();
    long[] due = new long[armed.length];
    long now = System.currentTimeMillis();
    for (int i = 0; i < armed.length; i++) {
      due[i] = AlarmSchedule.nextFireUtcMs(armed[i], now, store.offsetMinutes());
    }
    schedule = new Schedule(armed, due);
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
    boolean[] armed = new boolean[AlarmStore.MAX_ALARMS];
    if (Clock.isSet(System.currentTimeMillis())) {
      Schedule s = schedule;
      for (int i = 0; i < s.armed.length; i++) {
        if (s.dueUtcMs[i] != AlarmSchedule.NEVER) {
          armed[s.armed[i].id] = arm(s.armed[i].id, s.dueUtcMs[i]);
        }
      }
      // A snoozed alarm is handled outside that loop because a one-shot
      // disarms itself when it first rings: by now it is absent from
      // `armed`, and its snooze would be the one thing nobody re-armed.
      if (snoozed != null && snoozeUntilUtcMs != 0) {
        armed[snoozed.id] = arm(snoozed.id, snoozeUntilUtcMs);
      }
    }
    for (int id = 0; id < AlarmStore.MAX_ALARMS; id++) {
      if (!armed[id]) {
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
   * The operation for alarm {@code id}. Two of these are equal when their ids are, which is what
   * lets {@link #plan} replace and cancel by id; the due time rides along as an extra so {@link
   * #ring} can tell a fire it should honour from one the clock jumped over.
   */
  private PendingIntent operation(int id, long atUtcMs) {
    Intent intent =
        new Intent(picoclock.ui.RingActivity.class)
            .putExtra(picoclock.ui.RingActivity.EXTRA_ALARM_ID, id)
            .putExtra(
                picoclock.ui.RingActivity.EXTRA_DUE_MINUTE, AlarmSchedule.epochMinute(atUtcMs));
    return PendingIntent.getActivity(
        this, id, intent, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
  }

  /**
   * Start ringing alarm {@code id}, which the framework has just brought a screen up for. Main
   * thread. Returns false when the fire is stale — the clock moved, or the alarm was turned off
   * between the arming and now — in which case the alarms have been re-planned and the caller
   * should take its screen away again.
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

    // A one-shot disarms itself once it has rung, as on Android. A snooze is
    // the same alarm ringing again, so it does not disarm anything twice.
    if (!a.repeats() && !fromSnooze) {
      a.enabled = false;
      store.save(id);
    }

    verdict = VERDICT_NONE; // a verdict left over from the ring this supersedes
    beat = 0;
    ringing = a; // published to the ticker, which starts sounding within a tick
    Log.i(TAG, "ring " + a.time() + (a.label.isEmpty() ? "" : " " + a.label));
    reload();
    return true;
  }

  /** The alarm currently ringing, or null. */
  public Alarm ringing() {
    return ringing;
  }

  /**
   * Silence the ring and re-arm it {@link AlarmStore#SNOOZE_MINUTES} out. Main thread. The buzzer
   * goes quiet on the next tick — it belongs to the ticker — and the screen calling this finishes
   * itself without waiting.
   */
  public void snooze() {
    Alarm up = ringing;
    if (up == null) {
      return;
    }
    snoozed = up;
    snoozeUntilUtcMs = System.currentTimeMillis() + AlarmStore.SNOOZE_MINUTES * Clock.MS_PER_MINUTE;
    Log.i(TAG, "snoozed " + up.time() + " for " + AlarmStore.SNOOZE_MINUTES + " min");
    verdict = VERDICT_SNOOZE;
    plan();
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
        tick();
      } catch (RuntimeException e) {
        // A fault here would leave the board with no alarms at all, silently.
        Log.w(TAG, "tick failed: " + e);
      }
    }
    // The buzzer belongs to this thread for its whole life, this end included.
    buzzer.off();
    buzzer.close();
  }

  /**
   * The heartbeat. It notifies the screens, sounds the buzzer while a ring stands, and applies
   * whatever a screen decided about that ring — but it does not decide when an alarm rings. The
   * framework does, and it goes on doing so with this app shut down.
   */
  private void tick() {
    Executors.mainExecutor().execute(tickNotify);
    applyVerdict();
    if (ringing != null) {
      buzzer.beat(beat++);
    }
  }

  /** Apply a screen's Snooze or Stop. Ticker only — this is where the buzzer goes quiet. */
  private void applyVerdict() {
    int v = verdict;
    if (v == VERDICT_NONE) {
      return;
    }
    verdict = VERDICT_NONE;
    Alarm up = ringing;
    if (up == null) {
      return; // the ring ended on its own between the tap and this tick
    }
    if (v == VERDICT_DISMISS) {
      Log.i(TAG, "dismissed " + up.time());
    }
    stopRing();
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
