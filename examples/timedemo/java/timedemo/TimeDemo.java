// SPDX-License-Identifier: GPL-3.0-only
package timedemo;

import java.time.DateTimeException;
import java.time.DayOfWeek;
import java.time.Duration;
import java.time.Instant;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.LocalTime;
import java.time.Month;
import java.time.ZoneId;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.time.temporal.ChronoUnit;
import java.util.TimeZone;
import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Self-checking exercise of the SDK's {@code java.time}. Every expectation outside {@link
 * #portSpecific} also holds on the JDK's own implementation: the port was validated by running this
 * same source on a host JDK, once against the real {@code java.time} and once with the SDK's
 * classes patched into {@code java.base}. {@link #portSpecific} covers the documented divergences:
 * no tz database, and a process default zone that starts at UTC.
 */
public class TimeDemo extends Application {
  private static final String TAG = "TimeDemo";

  static int passed = 0;
  static int failed = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      Log.i(TAG, "PASS: " + name);
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  /** A value Error Prone cannot fold: its InvalidJavaTimeConstant check reads literals only. */
  static int dyn(int v) {
    return v;
  }

  /** Swallows a result whose only purpose was to throw on the way here. */
  static void sink(Object o) {
    if (o == null) {
      failed = failed + 1;
    }
  }

  static boolean throwsDateTime(Runnable r) {
    try {
      r.run();
      return false;
    } catch (DateTimeException e) {
      return true;
    }
  }

  static boolean throwsArithmetic(Runnable r) {
    try {
      r.run();
      return false;
    } catch (ArithmeticException e) {
      return true;
    }
  }

  static boolean throwsIllegalArgument(Runnable r) {
    try {
      r.run();
      return false;
    } catch (IllegalArgumentException e) {
      return true;
    }
  }

  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    Log.i(TAG, "=== java.time Tests ===");
    passed = 0;
    failed = 0;

    dates();
    times();
    dateTimes();
    instantsAndDurations();
    zones();
    formatting();
    maths();
    portSpecific();

    Log.i(TAG, "passed=" + passed + " failed=" + failed);
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    }
  }

  static void dates() {
    LocalDate d = LocalDate.of(2026, 9, 23);
    check("day of week", d.getDayOfWeek() == DayOfWeek.WEDNESDAY);
    check("epoch day", d.toEpochDay() == 20719);
    check("ofEpochDay round trip", LocalDate.ofEpochDay(d.toEpochDay()).equals(d));
    check("date toString", d.toString().equals("2026-09-23"));
    check(
        "getters",
        d.getYear() == 2026 && d.getMonth() == Month.SEPTEMBER && d.getDayOfMonth() == 23);
    check(
        "plusMonths clamps",
        LocalDate.of(2026, 1, 31).plusMonths(1).equals(LocalDate.of(2026, 2, 28)));
    check(
        "leap year",
        LocalDate.of(2024, 2, 29).isLeapYear()
            && LocalDate.of(2024, 2, 29).plusYears(1).equals(LocalDate.of(2025, 2, 28)));
    check("day of year", LocalDate.of(2024, 12, 31).getDayOfYear() == 366);
    check("ofYearDay", LocalDate.ofYearDay(2024, 60).equals(LocalDate.of(2024, 2, 29)));
    check(
        "minusDays across year",
        LocalDate.of(2026, 1, 1).minusDays(1).equals(LocalDate.of(2025, 12, 31)));
    check(
        "plusDays far", LocalDate.of(2026, 9, 23).plusDays(1000).equals(LocalDate.of(2029, 6, 19)));
    check("negative epoch day", LocalDate.ofEpochDay(-1).equals(LocalDate.of(1969, 12, 31)));
    check("year 1900 dow", LocalDate.of(1900, 1, 1).getDayOfWeek() == DayOfWeek.MONDAY);
    check("parse date", LocalDate.parse("2026-09-23").equals(d));
    check(
        "date compare",
        LocalDate.of(2026, 9, 22).isBefore(d)
            && d.isAfter(LocalDate.of(2026, 9, 22))
            && d.compareTo(LocalDate.of(2026, 9, 23)) == 0
            && d.isEqual(LocalDate.of(2026, 9, 23)));
    check("until days", LocalDate.of(2026, 1, 1).until(d, ChronoUnit.DAYS) == 265);
    check(
        "until months",
        LocalDate.of(2026, 1, 31).until(LocalDate.of(2026, 3, 30), ChronoUnit.MONTHS) == 1);
    check("DAYS.between", ChronoUnit.DAYS.between(d, LocalDate.of(2026, 9, 30)) == 7);
    check("WEEKS.between", ChronoUnit.WEEKS.between(LocalDate.of(2026, 1, 1), d) == 37);
    check("plus unit", d.plus(2, ChronoUnit.WEEKS).equals(LocalDate.of(2026, 10, 7)));
    check("withDayOfMonth", d.withDayOfMonth(1).equals(LocalDate.of(2026, 9, 1)));
    check(
        "lengthOfMonth", d.lengthOfMonth() == 30 && LocalDate.of(2024, 2, 1).lengthOfMonth() == 29);
    check("invalid date throws", throwsDateTime(() -> sink(LocalDate.of(2026, 2, dyn(29)))));
    check("invalid month throws", throwsDateTime(() -> sink(LocalDate.of(2026, dyn(13), 1))));
    check("atStartOfDay", d.atStartOfDay().equals(LocalDateTime.of(2026, 9, 23, 0, 0)));
    check("Month.of", Month.of(2).length(true) == 29 && Month.FEBRUARY.plus(11) == Month.JANUARY);
    check("Month.firstDayOfYear", Month.MARCH.firstDayOfYear(true) == 61);
    check(
        "DayOfWeek arithmetic",
        DayOfWeek.SUNDAY.plus(1) == DayOfWeek.MONDAY
            && DayOfWeek.MONDAY.minus(1) == DayOfWeek.SUNDAY
            && DayOfWeek.of(7) == DayOfWeek.SUNDAY);
  }

  static void times() {
    LocalTime t = LocalTime.of(23, 59, 30);
    check("plusMinutes wraps", t.plusMinutes(1).equals(LocalTime.of(0, 0, 30)));
    check("minusHours wraps", LocalTime.of(1, 0).minusHours(2).equals(LocalTime.of(23, 0)));
    check("time toString hm", LocalTime.of(9, 5).toString().equals("09:05"));
    check("time toString hms", t.toString().equals("23:59:30"));
    check(
        "time toString nanos",
        LocalTime.of(1, 2, 3, 500_000_000).toString().equals("01:02:03.500"));
    check("secondOfDay", t.toSecondOfDay() == 86370);
    check("ofSecondOfDay", LocalTime.ofSecondOfDay(3661).equals(LocalTime.of(1, 1, 1)));
    check("parse time", LocalTime.parse("07:08:09").equals(LocalTime.of(7, 8, 9)));
    check("parse time hm", LocalTime.parse("07:08").equals(LocalTime.of(7, 8)));
    check(
        "until minutes",
        LocalTime.of(10, 0).until(LocalTime.of(12, 30), ChronoUnit.MINUTES) == 150);
    check("until negative", LocalTime.of(12, 0).until(LocalTime.of(10, 0), ChronoUnit.HOURS) == -2);
    check(
        "truncatedTo minutes",
        LocalTime.of(10, 15, 45).truncatedTo(ChronoUnit.MINUTES).equals(LocalTime.of(10, 15)));
    check(
        "time compare",
        LocalTime.NOON.isAfter(LocalTime.MIDNIGHT) && LocalTime.MIN.isBefore(LocalTime.MAX));
    check("withHour", t.withHour(1).equals(LocalTime.of(1, 59, 30)));
    check("invalid hour throws", throwsDateTime(() -> sink(LocalTime.of(dyn(24), 0))));
  }

  static void dateTimes() {
    LocalDateTime dt = LocalDateTime.ofEpochSecond(1_000_000_000L, 0, ZoneOffset.UTC);
    check("ofEpochSecond fields", dt.equals(LocalDateTime.of(2001, 9, 9, 1, 46, 40)));
    check("toEpochSecond round trip", dt.toEpochSecond(ZoneOffset.UTC) == 1_000_000_000L);
    check(
        "positive offset",
        LocalDateTime.ofEpochSecond(1_000_000_000L, 0, ZoneOffset.ofHoursMinutes(5, 30))
            .equals(LocalDateTime.of(2001, 9, 9, 7, 16, 40)));
    check(
        "negative offset across midnight",
        LocalDateTime.ofEpochSecond(0, 0, ZoneOffset.ofHours(-5))
            .equals(LocalDateTime.of(1969, 12, 31, 19, 0)));
    check(
        "plusHours across day",
        LocalDateTime.of(2026, 9, 23, 23, 0)
            .plusHours(2)
            .equals(LocalDateTime.of(2026, 9, 24, 1, 0)));
    check(
        "minusSeconds across day",
        LocalDateTime.of(2026, 9, 24, 0, 0)
            .minusSeconds(1)
            .equals(LocalDateTime.of(2026, 9, 23, 23, 59, 59)));
    check(
        "plusMinutes many days",
        LocalDateTime.of(2026, 9, 23, 12, 0)
            .plusMinutes(3 * 1440 + 5)
            .equals(LocalDateTime.of(2026, 9, 26, 12, 5)));
    check(
        "datetime toString",
        LocalDateTime.of(2026, 9, 23, 14, 5, 9).toString().equals("2026-09-23T14:05:09"));
    check(
        "parse datetime",
        LocalDateTime.parse("2026-09-23T14:05").equals(LocalDateTime.of(2026, 9, 23, 14, 5)));
    check(
        "until hours",
        LocalDateTime.of(2026, 9, 23, 22, 0)
                .until(LocalDateTime.of(2026, 9, 24, 3, 30), ChronoUnit.HOURS)
            == 5);
    check(
        "until days partial",
        LocalDateTime.of(2026, 9, 23, 22, 0)
                .until(LocalDateTime.of(2026, 9, 24, 21, 0), ChronoUnit.DAYS)
            == 0);
    check(
        "until minutes negative",
        LocalDateTime.of(2026, 9, 24, 0, 10)
                .until(LocalDateTime.of(2026, 9, 23, 23, 50), ChronoUnit.MINUTES)
            == -20);
    check(
        "ofInstant",
        LocalDateTime.ofInstant(Instant.ofEpochMilli(1_000_000_000_000L), ZoneOffset.UTC)
            .equals(LocalDateTime.of(2001, 9, 9, 1, 46, 40)));
    check("toInstant", dt.toInstant(ZoneOffset.UTC).equals(Instant.ofEpochSecond(1_000_000_000L)));
    check(
        "datetime compare",
        LocalDateTime.of(2026, 9, 23, 0, 0).isBefore(LocalDateTime.of(2026, 9, 23, 0, 1))
            && dt.compareTo(LocalDateTime.ofEpochSecond(1_000_000_000L, 0, ZoneOffset.UTC)) == 0);
    check(
        "toLocalDate/Time",
        dt.toLocalDate().equals(LocalDate.of(2001, 9, 9))
            && dt.toLocalTime().equals(LocalTime.of(1, 46, 40)));
    check(
        "of(date, time)",
        LocalDateTime.of(LocalDate.of(2026, 9, 23), LocalTime.NOON).getHour() == 12);
  }

  static void instantsAndDurations() {
    Instant i = Instant.ofEpochMilli(-1);
    check("negative millis", i.getEpochSecond() == -1 && i.getNano() == 999_000_000);
    check("toEpochMilli round trip", i.toEpochMilli() == -1);
    check("plus duration", Instant.EPOCH.plus(Duration.ofMinutes(90)).getEpochSecond() == 5400);
    check(
        "plusMillis", Instant.EPOCH.plusMillis(1500).equals(Instant.ofEpochSecond(1, 500_000_000)));
    check(
        "instant toString",
        Instant.ofEpochSecond(1_000_000_000L).toString().equals("2001-09-09T01:46:40Z"));
    check(
        "instant compare",
        Instant.EPOCH.isBefore(Instant.ofEpochSecond(1))
            && Instant.ofEpochSecond(1).isAfter(Instant.EPOCH));
    check(
        "instant until",
        Instant.EPOCH.until(Instant.ofEpochSecond(3600), ChronoUnit.MINUTES) == 60);
    check("now after epoch", !Instant.now().isBefore(Instant.EPOCH));

    Duration dur =
        Duration.between(
            Instant.ofEpochSecond(10, 500_000_000), Instant.ofEpochSecond(12, 200_000_000));
    check("between", dur.equals(Duration.ofMillis(1700)));
    check(
        "between negative",
        Duration.between(LocalTime.of(12, 0), LocalTime.of(11, 59, 59))
            .equals(Duration.ofSeconds(-1)));
    check(
        "between datetimes",
        Duration.between(LocalDateTime.of(2026, 9, 23, 23, 0), LocalDateTime.of(2026, 9, 24, 1, 30))
            .equals(Duration.ofMinutes(150)));
    check("duration toString", Duration.ofSeconds(3661).toString().equals("PT1H1M1S"));
    check("duration toString millis", Duration.ofMillis(1500).toString().equals("PT1.5S"));
    check("duration toString negative", Duration.ofSeconds(-90).toString().equals("PT-1M-30S"));
    check("duration toString zero", Duration.ZERO.toString().equals("PT0S"));
    check("toMinutes", Duration.ofHours(2).plusMinutes(30).toMinutes() == 150);
    check("toMillis", Duration.ofSeconds(1, 500_000_000).toMillis() == 1500);
    check("dividedBy", Duration.ofSeconds(10).dividedBy(4).equals(Duration.ofMillis(2500)));
    check("multipliedBy", Duration.ofMillis(300).multipliedBy(4).equals(Duration.ofMillis(1200)));
    check(
        "negated abs",
        Duration.ofSeconds(5).negated().isNegative()
            && Duration.ofSeconds(-5).abs().getSeconds() == 5);
    check("ofSeconds normalises", Duration.ofSeconds(1, -1).equals(Duration.ofNanos(999_999_999)));
    check("duration compare", Duration.ofSeconds(1).compareTo(Duration.ofMillis(999)) > 0);
    check("of unit", Duration.of(3, ChronoUnit.HOURS).equals(Duration.ofHours(3)));
    check(
        "overflow throws",
        throwsArithmetic(() -> sink(Duration.ofSeconds(Long.MAX_VALUE).plusSeconds(1))));
  }

  static void zones() {
    check("offset id", ZoneOffset.ofHoursMinutes(5, 30).getId().equals("+05:30"));
    check("offset parse", ZoneOffset.of("-08:00").getTotalSeconds() == -8 * 3600);
    check("offset parse short", ZoneOffset.of("+1").getTotalSeconds() == 3600);
    check("offset Z", ZoneOffset.of("Z").equals(ZoneOffset.UTC));
    check("offset equals", ZoneOffset.ofHours(2).equals(ZoneOffset.ofTotalSeconds(7200)));
    check("zone of UTC", ZoneId.of("UTC").getId().equals("UTC"));
    check(
        "zone of UTC rules",
        ZoneId.of("UTC").getRules().getOffset(Instant.EPOCH).equals(ZoneOffset.UTC));
    check(
        "zone of UTC+1",
        ZoneId.of("UTC+01:00").getRules().getOffset(Instant.EPOCH).equals(ZoneOffset.ofHours(1)));
    check("zone of offset", ZoneId.of("-03:00").equals(ZoneOffset.ofHours(-3)));
    check(
        "ofInstant in named zone",
        LocalDateTime.ofInstant(Instant.EPOCH, ZoneId.of("GMT+02:00")).getHour() == 2);
    check("bad offset throws", throwsDateTime(() -> sink(ZoneOffset.ofHours(dyn(19)))));
    TimeZone saved = TimeZone.getDefault();
    TimeZone.setDefault(TimeZone.getTimeZone("GMT+05:30"));
    check(
        "systemDefault follows setDefault",
        ZoneId.systemDefault()
            .getRules()
            .getOffset(Instant.EPOCH)
            .equals(ZoneOffset.ofHoursMinutes(5, 30)));
    check("TimeZone raw offset", TimeZone.getDefault().getRawOffset() == 19_800_000);
    check(
        "TimeZone toZoneId",
        TimeZone.getDefault().toZoneId().getRules().getOffset(Instant.EPOCH).getTotalSeconds()
            == 19_800);
    check(
        "now in default zone",
        LocalDateTime.now(ZoneId.systemDefault()).toLocalTime().toSecondOfDay() >= 0
            && !LocalDate.now(ZoneId.systemDefault()).isBefore(LocalDate.of(1970, 1, 1)));
    TimeZone.setDefault(saved);
  }

  /** What the port does where the JDK, with its tz database and host zone, does otherwise. */
  static void portSpecific() {
    check("region id throws", throwsDateTime(() -> sink(ZoneId.of("Europe/London"))));
    check("unknown TimeZone is GMT", TimeZone.getTimeZone("Europe/London").getRawOffset() == 0);
    TimeZone.setDefault(null);
    check("default zone starts at UTC", ZoneId.systemDefault().equals(ZoneOffset.UTC));
  }

  static void formatting() {
    LocalDateTime dt = LocalDateTime.of(2026, 9, 23, 14, 5);
    DateTimeFormatter f = DateTimeFormatter.ofPattern("EEE, d MMM yyyy HH:mm");
    check("format pattern", dt.format(f).equals("Wed, 23 Sep 2026 14:05"));
    check("format via formatter", f.format(dt).equals("Wed, 23 Sep 2026 14:05"));
    check(
        "format 12h",
        DateTimeFormatter.ofPattern("h:mm a").format(LocalTime.of(0, 7)).equals("12:07 AM"));
    check(
        "format pm",
        DateTimeFormatter.ofPattern("hh:mm a").format(LocalTime.of(13, 7)).equals("01:07 PM"));
    check(
        "format literal",
        DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss")
            .format(LocalDateTime.of(2001, 9, 9, 1, 46, 40))
            .equals("2001-09-09T01:46:40"));
    check(
        "format yy",
        DateTimeFormatter.ofPattern("dd/MM/yy")
            .format(LocalDate.of(2026, 1, 5))
            .equals("05/01/26"));
    check(
        "format full names",
        DateTimeFormatter.ofPattern("EEEE d MMMM")
            .format(LocalDate.of(2026, 9, 23))
            .equals("Wednesday 23 September"));
    check(
        "format fraction",
        DateTimeFormatter.ofPattern("ss.SSS")
            .format(LocalTime.of(0, 0, 7, 120_000_000))
            .equals("07.120"));
    check(
        "format day of year",
        DateTimeFormatter.ofPattern("D").format(LocalDate.of(2026, 2, 1)).equals("32"));
    check(
        "ISO_LOCAL_DATE",
        LocalDate.of(2026, 9, 23).format(DateTimeFormatter.ISO_LOCAL_DATE).equals("2026-09-23"));
    check(
        "ISO_LOCAL_TIME",
        LocalTime.of(9, 5, 0).format(DateTimeFormatter.ISO_LOCAL_TIME).equals("09:05:00"));
    check(
        "bad letter throws",
        throwsIllegalArgument(() -> sink(DateTimeFormatter.ofPattern("yyyy b"))));
    check(
        "date field on time throws",
        throwsDateTime(() -> sink(DateTimeFormatter.ofPattern("yyyy").format(LocalTime.NOON))));
  }

  static void maths() {
    check("floorDiv int", Math.floorDiv(-7, 2) == -4 && Math.floorMod(-7, 2) == 1);
    check("floorDiv long", Math.floorDiv(-7L, 2L) == -4L && Math.floorMod(7L, -2L) == -1L);
    check("floorMod positive", Math.floorMod(7, 2) == 1 && Math.floorDiv(7, 2) == 3);
    check("addExact", Math.addExact(1, 2) == 3 && Math.addExact(1L, 2L) == 3L);
    check("multiplyExact", Math.multiplyExact(1L << 20, 1L << 20) == (1L << 40));
    check("addExact throws", throwsArithmetic(() -> sink(Math.addExact(Integer.MAX_VALUE, 1))));
    check(
        "multiplyExact throws",
        throwsArithmetic(() -> sink(Math.multiplyExact(1L << 40, 1L << 40))));
    check(
        "toIntExact",
        Math.toIntExact(42L) == 42 && throwsArithmetic(() -> sink(Math.toIntExact(1L << 40))));
  }
}
