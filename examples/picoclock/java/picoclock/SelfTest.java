// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import picodroid.util.Log;

/**
 * Checks the clock's arithmetic against hand-computed instants at startup, and logs one pass/fail
 * line. Every screen in this app is a rendering of {@link Clock} and {@link AlarmSchedule}, and
 * neither can be exercised by tapping at a panel: an off-by-one in the day-of-week or a repeat that
 * schedules itself into the past shows up days later as an alarm that did not ring. This is what
 * the sim and HIL rows assert on.
 *
 * <p>Dates below are UTC instants taken from the proleptic Gregorian calendar: 2026-09-10T00:00:00Z
 * is epoch day 20706, a Thursday.
 */
public final class SelfTest {
  private static final String TAG = ClockApp.TAG;

  /** 2026-09-10T00:00:00Z. */
  private static final long SEP_10_2026 = 20_706L * Clock.MS_PER_DAY;

  private static int checks;
  private static int failures;

  private SelfTest() {}

  /** Runs every check and logs the result. Returns whether they all passed. */
  public static boolean run() {
    checks = 0;
    failures = 0;
    civil();
    weekdays();
    formatting();
    oneShot();
    repeating();
    lateness();
    across();
    if (failures == 0) {
      Log.i(TAG, "selftest === ALL PASSED === (" + checks + " checks)");
    } else {
      Log.w(TAG, "selftest === FAILED === " + failures + " of " + checks);
    }
    return failures == 0;
  }

  // ── Calendar ───────────────────────────────────────────────────────────────

  private static void civil() {
    int[] ymd = Clock.civilFromDay(20_706L);
    check("civil 20706 = 2026-09-10", ymd[0] == 2026 && ymd[1] == 9 && ymd[2] == 10);
    check("day 0 = 1970-01-01", eq(Clock.civilFromDay(0), 1970, 1, 1));
    check("day -1 = 1969-12-31", eq(Clock.civilFromDay(-1), 1969, 12, 31));
    check("leap 2024-02-29", eq(Clock.civilFromDay(Clock.dayFromCivil(2024, 2, 29)), 2024, 2, 29));
    check("round trip", Clock.dayFromCivil(2026, 9, 10) == 20_706L);
    check("round trip pre-epoch", Clock.dayFromCivil(1969, 7, 20) == -165L);

    check("Feb 2024 has 29", Clock.daysInMonth(2024, 2) == 29);
    check("Feb 2025 has 28", Clock.daysInMonth(2025, 2) == 28);
    check("Feb 1900 has 28", Clock.daysInMonth(1900, 2) == 28);
    check("Feb 2000 has 29", Clock.daysInMonth(2000, 2) == 29);
    check("Dec has 31", Clock.daysInMonth(2026, 12) == 31);
    check("Apr has 30", Clock.daysInMonth(2026, 4) == 30);

    // Negative epoch ms must floor, not truncate, or every pre-1970 date is a day off.
    check("floorDiv negative", Clock.floorDiv(-1, 86_400_000L) == -1);
    check("floorMod negative", Clock.floorMod(-1, 86_400_000L) == 86_399_999L);
    check("msIntoDay negative", Clock.msIntoDay(-1) == 86_399_999L);
  }

  private static void weekdays() {
    check("1970-01-01 was a Thursday", Clock.dayOfWeek(0) == 4);
    check("2026-09-10 is a Thursday", Clock.dayOfWeek(20_706L) == 4);
    check("the day after is a Friday", Clock.dayOfWeek(20_707L) == 5);
    check("six days on is a Wednesday", Clock.dayOfWeek(20_712L) == 3);
    check("a week on is a Thursday again", Clock.dayOfWeek(20_713L) == 4);
    check("1969-12-31 was a Wednesday", Clock.dayOfWeek(-1) == 3);
  }

  private static void formatting() {
    long noon = SEP_10_2026 + 12 * Clock.MS_PER_HOUR + 34 * Clock.MS_PER_MINUTE + 56 * 1000L;
    check("hour", Clock.hourOf(noon) == 12);
    check("minute", Clock.minuteOf(noon) == 34);
    check("second", Clock.secondOf(noon) == 56);
    check("hm pads", Clock.hm(7, 5).equals("07:05"));
    check("date", Clock.date(noon).equals("Thu 10 Sep 2026"));
    check("offset zero", Clock.offset(0).equals("UTC"));
    check("offset whole hours", Clock.offset(-480).equals("UTC-8"));
    check("offset half hours", Clock.offset(330).equals("UTC+5:30"));
    check("until minutes", Clock.until(7 * Clock.MS_PER_MINUTE).equals("in 7 min"));
    check(
        "until hours",
        Clock.until(3 * Clock.MS_PER_HOUR + 20 * Clock.MS_PER_MINUTE).equals("in 3 h 20 min"));
    check("until whole hours", Clock.until(5 * Clock.MS_PER_HOUR).equals("in 5 h"));
    check("until days", Clock.until(2 * Clock.MS_PER_DAY).equals("in 2 days"));
    check("until one day", Clock.until(Clock.MS_PER_DAY).equals("in 1 day"));
    check("until seconds", Clock.until(30 * 1000L).equals("in under a minute"));

    check("unset clock at boot", !Clock.isSet(5 * Clock.MS_PER_MINUTE));
    check("set clock once synced", Clock.isSet(SEP_10_2026));

    // The offset shifts the displayed civil date, not just the time.
    long lateUtc = SEP_10_2026 + 23 * Clock.MS_PER_HOUR;
    check(
        "offset rolls the date over",
        Clock.date(Clock.toLocal(lateUtc, 120)).equals("Fri 11 Sep 2026"));
    check("toUtc inverts toLocal", Clock.toUtc(Clock.toLocal(lateUtc, 330), 330) == lateUtc);
  }

  // ── Scheduling ─────────────────────────────────────────────────────────────

  private static void oneShot() {
    Alarm a = alarm(7, 0, Alarm.ONCE);
    long eightAm = SEP_10_2026 + 8 * Clock.MS_PER_HOUR;

    check(
        "one-shot already past today rolls to tomorrow",
        AlarmSchedule.nextFireUtcMs(a, eightAm, 0)
            == SEP_10_2026 + Clock.MS_PER_DAY + 7 * Clock.MS_PER_HOUR);

    long sixAm = SEP_10_2026 + 6 * Clock.MS_PER_HOUR;
    check(
        "one-shot still ahead fires today",
        AlarmSchedule.nextFireUtcMs(a, sixAm, 0) == SEP_10_2026 + 7 * Clock.MS_PER_HOUR);

    // Standing exactly on the alarm means the *next* one, never the one that just rang.
    check(
        "one-shot standing on it rolls over",
        AlarmSchedule.nextFireUtcMs(a, SEP_10_2026 + 7 * Clock.MS_PER_HOUR, 0)
            == SEP_10_2026 + Clock.MS_PER_DAY + 7 * Clock.MS_PER_HOUR);

    a.enabled = false;
    check("disarmed never fires", AlarmSchedule.nextFireUtcMs(a, sixAm, 0) == AlarmSchedule.NEVER);
  }

  private static void repeating() {
    // Thursday 06:00 UTC, alarm at 07:00 on weekdays.
    long sixAmThu = SEP_10_2026 + 6 * Clock.MS_PER_HOUR;
    int weekdays = Alarm.MONDAY | Alarm.TUESDAY | Alarm.WEDNESDAY | Alarm.THURSDAY | Alarm.FRIDAY;
    Alarm a = alarm(7, 0, weekdays);

    check(
        "weekday alarm fires today",
        AlarmSchedule.nextFireUtcMs(a, sixAmThu, 0) == SEP_10_2026 + 7 * Clock.MS_PER_HOUR);

    long eightAmFri = SEP_10_2026 + Clock.MS_PER_DAY + 8 * Clock.MS_PER_HOUR;
    check(
        "past Friday's, skips the weekend to Monday",
        AlarmSchedule.nextFireUtcMs(a, eightAmFri, 0)
            == SEP_10_2026 + 4 * Clock.MS_PER_DAY + 7 * Clock.MS_PER_HOUR);

    Alarm sundays = alarm(9, 30, Alarm.SUNDAY);
    check(
        "a once-weekly alarm reaches its day",
        AlarmSchedule.nextFireUtcMs(sundays, sixAmThu, 0)
            == SEP_10_2026
                + 3 * Clock.MS_PER_DAY
                + 9 * Clock.MS_PER_HOUR
                + 30 * Clock.MS_PER_MINUTE);

    // Standing on a weekly alarm must give next week, which is the eighth day
    // the scan looks at — the case a seven-day scan gets wrong.
    long sundayNineThirty =
        SEP_10_2026 + 3 * Clock.MS_PER_DAY + 9 * Clock.MS_PER_HOUR + 30 * Clock.MS_PER_MINUTE;
    check(
        "standing on a weekly alarm gives next week",
        AlarmSchedule.nextFireUtcMs(sundays, sundayNineThirty, 0)
            == sundayNineThirty + 7 * Clock.MS_PER_DAY);

    // The mask is read in local days, not UTC ones. At UTC+5:30, Thursday
    // 20:00 UTC is already Friday 01:30 locally, so a Friday alarm at 22:00
    // rings that same evening — a UTC reading of the mask would wait a day.
    Alarm fridayEvening = alarm(22, 0, Alarm.FRIDAY);
    long thu2000Utc = SEP_10_2026 + 20 * Clock.MS_PER_HOUR;
    long friday = SEP_10_2026 + Clock.MS_PER_DAY; // local midnight, Friday
    check(
        "repeat days follow the local calendar",
        AlarmSchedule.nextFireUtcMs(fridayEvening, thu2000Utc, 330)
            == Clock.toUtc(friday + 22 * Clock.MS_PER_HOUR, 330));
  }

  private static void lateness() {
    long due = SEP_10_2026 + 7 * Clock.MS_PER_HOUR;
    check("rings on the second", AlarmSchedule.shouldRing(due, due));
    check("rings a tick late", AlarmSchedule.shouldRing(due, due + 250));
    check(
        "rings within the tolerance",
        AlarmSchedule.shouldRing(due, due + AlarmSchedule.LATE_TOLERANCE_MS));
    check("does not ring before it is due", !AlarmSchedule.shouldRing(due, due - 1));
    check(
        "does not ring for a clock that jumped over it",
        !AlarmSchedule.shouldRing(due, due + Clock.MS_PER_HOUR));
    check("never never rings", !AlarmSchedule.shouldRing(AlarmSchedule.NEVER, due));
  }

  private static void across() {
    long sixAm = SEP_10_2026 + 6 * Clock.MS_PER_HOUR;
    Alarm early = alarm(6, 30, Alarm.ONCE);
    Alarm late = alarm(7, 0, Alarm.ONCE);
    Alarm off = alarm(6, 15, Alarm.ONCE);
    off.enabled = false;
    Alarm[] all = {late, off, early};

    check(
        "soonest wins",
        AlarmSchedule.nextFireUtcMs(all, sixAm, 0)
            == SEP_10_2026 + 6 * Clock.MS_PER_HOUR + 30 * Clock.MS_PER_MINUTE);
    check("and names its alarm", AlarmSchedule.nextAlarm(all, sixAm, 0) == early);
    check(
        "no alarms at all",
        AlarmSchedule.nextFireUtcMs(new Alarm[0], sixAm, 0) == AlarmSchedule.NEVER);
    check("all disarmed", AlarmSchedule.nextAlarm(new Alarm[] {off}, sixAm, 0) == null);

    check("repeat text once", alarm(7, 0, Alarm.ONCE).repeatText().equals("Once"));
    check("repeat text daily", alarm(7, 0, Alarm.EVERY_DAY).repeatText().equals("Every day"));
    check(
        "repeat text is Monday-first",
        alarm(7, 0, Alarm.SUNDAY | Alarm.MONDAY).repeatText().equals("Mon Sun"));
  }

  // ── Plumbing ───────────────────────────────────────────────────────────────

  private static Alarm alarm(int hour, int minute, int daysMask) {
    Alarm a = new Alarm(0);
    a.hour = hour;
    a.minute = minute;
    a.daysMask = daysMask;
    a.enabled = true;
    return a;
  }

  private static boolean eq(int[] ymd, int year, int month, int day) {
    return ymd[0] == year && ymd[1] == month && ymd[2] == day;
  }

  private static void check(String what, boolean ok) {
    checks++;
    if (!ok) {
      failures++;
      Log.w(TAG, "selftest FAIL: " + what);
    }
  }
}
