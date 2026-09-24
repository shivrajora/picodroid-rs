// SPDX-License-Identifier: GPL-3.0-only
package java.util;

import java.time.Instant;
import java.time.ZoneId;
import java.time.ZoneOffset;

/**
 * A fixed-offset time zone. There is no tz database on the device, so a zone is an offset from UTC
 * and never observes daylight saving. {@link #getTimeZone(String)} understands {@code GMT}, {@code
 * UTC} and {@code GMT+hh:mm} and, like the JDK, falls back to GMT for an id it does not know.
 * {@link #setDefault} installs the zone {@link ZoneId#systemDefault()} and the {@code now()}
 * methods of {@code java.time} use: an app that learns its offset from the network (SNTP carries
 * none; a bridge or a settings screen can) sets it here once.
 */
public class TimeZone {
  private static TimeZone defaultZone;

  private final String id;
  private final int rawOffsetMillis;

  TimeZone(String id, int rawOffsetMillis) {
    this.id = id;
    this.rawOffsetMillis = rawOffsetMillis;
  }

  /** The process default, UTC until {@link #setDefault} is called. */
  public static TimeZone getDefault() {
    if (defaultZone == null) {
      defaultZone = new TimeZone("UTC", 0);
    }
    return defaultZone;
  }

  /** Installs {@code zone} as the default; {@code null} restores UTC. */
  public static void setDefault(TimeZone zone) {
    defaultZone = zone;
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
}
