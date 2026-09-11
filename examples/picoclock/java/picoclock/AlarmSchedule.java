// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

/**
 * When an alarm fires next. Pure arithmetic over {@link Clock} — no state, no clock read of its own
 * — so {@link SelfTest} can check it against hand-computed instants, which is the only way to test
 * a scheduler on a board whose wall clock cannot be moved without moving the real one.
 */
public final class AlarmSchedule {
  /** No alarm is due: the value {@link #nextFireUtcMs} returns for a disarmed alarm. */
  public static final long NEVER = Long.MAX_VALUE;

  /**
   * How late a fire may be and still ring. A tick that lands a second or two past the instant is
   * normal; one that lands an hour past it means the wall clock jumped forward (a sync, or the user
   * setting the time), and ringing then would be an alarm for a moment that never happened.
   */
  public static final long LATE_TOLERANCE_MS = 60 * Clock.MS_PER_SECOND;

  private AlarmSchedule() {}

  /**
   * The first instant at or after {@code afterUtcMs} at which {@code alarm} rings, or {@link
   * #NEVER} if it is disarmed. Strictly after, so an alarm that has just fired schedules its next
   * occurrence rather than the one it is standing on.
   *
   * <p>A repeating alarm scans the next eight local days: seven covers every weekday and the eighth
   * covers the case where today's occurrence has already passed.
   */
  public static long nextFireUtcMs(Alarm alarm, long afterUtcMs, int offsetMinutes) {
    if (!alarm.enabled) {
      return NEVER;
    }
    long localNow = Clock.toLocal(afterUtcMs, offsetMinutes);
    long today = Clock.dayOf(localNow);
    long timeIntoDay = alarm.hour * Clock.MS_PER_HOUR + alarm.minute * Clock.MS_PER_MINUTE;

    if (!alarm.repeats()) {
      long candidate = today * Clock.MS_PER_DAY + timeIntoDay;
      if (candidate <= localNow) {
        candidate += Clock.MS_PER_DAY;
      }
      return Clock.toUtc(candidate, offsetMinutes);
    }

    for (int i = 0; i < 8; i++) {
      long day = today + i;
      if ((alarm.daysMask & (1 << Clock.dayOfWeek(day))) == 0) {
        continue;
      }
      long candidate = day * Clock.MS_PER_DAY + timeIntoDay;
      if (candidate > localNow) {
        return Clock.toUtc(candidate, offsetMinutes);
      }
    }
    return NEVER; // unreachable for a non-zero mask; a disarmed-shaped answer beats an exception
  }

  /**
   * The soonest fire across {@code alarms}, or {@link #NEVER} if none is armed. Ties go to the
   * lower id, which is the order the list screen shows.
   */
  public static long nextFireUtcMs(Alarm[] alarms, long afterUtcMs, int offsetMinutes) {
    long soonest = NEVER;
    for (int i = 0; i < alarms.length; i++) {
      long at = nextFireUtcMs(alarms[i], afterUtcMs, offsetMinutes);
      if (at < soonest) {
        soonest = at;
      }
    }
    return soonest;
  }

  /** The alarm that owns {@link #nextFireUtcMs}'s answer, or {@code null} if none is armed. */
  public static Alarm nextAlarm(Alarm[] alarms, long afterUtcMs, int offsetMinutes) {
    Alarm soonest = null;
    long at = NEVER;
    for (int i = 0; i < alarms.length; i++) {
      long candidate = nextFireUtcMs(alarms[i], afterUtcMs, offsetMinutes);
      if (candidate < at) {
        at = candidate;
        soonest = alarms[i];
      }
    }
    return soonest;
  }

  /**
   * Whether a fire scheduled for {@code dueUtcMs} should ring at {@code nowUtcMs}: due, and not so
   * long past due that the wall clock must have jumped over it. See {@link #LATE_TOLERANCE_MS}.
   */
  public static boolean shouldRing(long dueUtcMs, long nowUtcMs) {
    return dueUtcMs != NEVER && nowUtcMs >= dueUtcMs && nowUtcMs - dueUtcMs <= LATE_TOLERANCE_MS;
  }
}
