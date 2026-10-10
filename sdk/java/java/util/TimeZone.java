// SPDX-License-Identifier: GPL-3.0-only
package java.util;

import java.time.Instant;
import java.time.ZoneId;
import java.time.ZoneOffset;

/**
 * A fixed-offset time zone. There is no tz database on the device, so a zone is an offset from UTC
 * and never observes daylight saving. {@link #getTimeZone(String)} understands {@code GMT}, {@code
 * UTC} and {@code GMT+hh:mm} and, like the JDK, falls back to GMT for an id it does not know.
 *
 * <p>{@link #getDefault()} is the platform zone: the offset the user picked in Settings → Date
 * &amp; time, kept by the framework across reboots and shared by every app, as on Android. It is
 * what {@link ZoneId#systemDefault()} and the {@code now()} methods of {@code java.time} use. Its
 * id is {@code UTC} or {@code GMT+05:30}. {@link #setDefault} overrides it for this process only,
 * as Android's does; nothing an app does changes the platform zone (that is {@code
 * AlarmManager.setTimeZone}, which Settings calls).
 */
public class TimeZone {
  /** The process override from {@link #setDefault}, or null while the platform zone applies. */
  private static TimeZone overrideZone;

  /** The platform zone as last read, rebuilt when its offset moves. */
  private static TimeZone platformZone;

  private final String id;
  private final int rawOffsetMillis;

  TimeZone(String id, int rawOffsetMillis) {
    this.id = id;
    this.rawOffsetMillis = rawOffsetMillis;
  }

  /** The platform zone, or the override installed with {@link #setDefault}. */
  public static TimeZone getDefault() {
    if (overrideZone != null) {
      return overrideZone;
    }
    int offsetMillis = nativeDefaultOffsetMinutes() * 60_000;
    if (platformZone == null || platformZone.rawOffsetMillis != offsetMillis) {
      platformZone = new TimeZone(gmtId(offsetMillis / 60_000), offsetMillis);
    }
    return platformZone;
  }

  /** Installs {@code zone} as this process's default; {@code null} restores the platform zone. */
  public static void setDefault(TimeZone zone) {
    overrideZone = zone;
  }

  public static TimeZone getTimeZone(String id) {
    if (id == null) {
      throw new NullPointerException("id");
    }
    try {
      return getTimeZone(ZoneId.of(id));
    } catch (java.time.DateTimeException e) {
      return new TimeZone("GMT", 0);
    }
  }

  public static TimeZone getTimeZone(ZoneId zoneId) {
    int seconds = zoneId.getRules().getOffset(Instant.EPOCH).getTotalSeconds();
    return new TimeZone(zoneId.getId(), seconds * 1000);
  }

  /** The id of a fixed offset: {@code UTC} for zero, else {@code GMT+hh:mm} / {@code GMT-hh:mm}. */
  static String gmtId(int offsetMinutes) {
    if (offsetMinutes == 0) {
      return "UTC";
    }
    int abs = offsetMinutes < 0 ? -offsetMinutes : offsetMinutes;
    int h = abs / 60;
    int m = abs % 60;
    return (offsetMinutes < 0 ? "GMT-" : "GMT+")
        + (h < 10 ? "0" : "")
        + h
        + (m < 10 ? ":0" : ":")
        + m;
  }

  public String getID() {
    return id;
  }

  /** The offset from UTC in milliseconds. */
  public int getRawOffset() {
    return rawOffsetMillis;
  }

  /** The offset at {@code date}: always the raw offset, there is no daylight saving. */
  public int getOffset(long date) {
    return rawOffsetMillis;
  }

  public boolean useDaylightTime() {
    return false;
  }

  public int getDSTSavings() {
    return 0;
  }

  public ZoneId toZoneId() {
    return ZoneOffset.ofTotalSeconds(rawOffsetMillis / 1000);
  }

  @Override
  public String toString() {
    return id;
  }

  /** The platform zone's offset, minutes east of UTC (the framework's `/system/time` store). */
  private static native int nativeDefaultOffsetMinutes();
}
