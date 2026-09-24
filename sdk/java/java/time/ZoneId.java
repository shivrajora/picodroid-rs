// SPDX-License-Identifier: GPL-3.0-only
package java.time;

import java.time.zone.ZoneRules;
import java.util.TimeZone;

/**
 * A time-zone id. There is no tz database on the device, so every zone is a fixed {@link
 * ZoneOffset}: {@link #of} accepts {@code Z}, {@code UTC}, {@code GMT}, {@code UT}, an offset such
 * as {@code +05:30} and the {@code UTC+05:30} forms, and throws {@link DateTimeException} for a
 * region id such as {@code Europe/London}. {@link #systemDefault()} is the offset installed with
 * {@link TimeZone#setDefault}, UTC until then.
 */
public abstract class ZoneId {
  ZoneId() {}

  public static ZoneId systemDefault() {
    return TimeZone.getDefault().toZoneId();
  }

  public static ZoneId of(String zoneId) {
    if (zoneId == null) {
      throw new NullPointerException("zoneId");
    }
    if (zoneId.equals("Z")) {
      return ZoneOffset.UTC;
    }
    char c = zoneId.charAt(0);
    if (c == '+' || c == '-') {
      return ZoneOffset.of(zoneId);
    }
    // "UTC", "GMT", "UT" and their offset forms keep their spelling as the id, as the JDK's do.
    if (zoneId.equals("UTC") || zoneId.equals("GMT") || zoneId.equals("UT")) {
      return new Fixed(zoneId, ZoneOffset.UTC);
    }
    if (zoneId.startsWith("UTC") || zoneId.startsWith("GMT")) {
      return new Fixed(zoneId, ZoneOffset.of(zoneId.substring(3)));
    }
    if (zoneId.startsWith("UT")) {
      return new Fixed(zoneId, ZoneOffset.of(zoneId.substring(2)));
    }
    throw new DateTimeException("Unknown time-zone ID: " + zoneId);
  }

  public abstract String getId();

  /** The zone's rules: one fixed offset, since there is no tz database. */
  public abstract ZoneRules getRules();

  /** A named zone with a fixed offset: {@code UTC}, {@code GMT+01:00}. */
  static final class Fixed extends ZoneId {
    private final String id;
    private final ZoneRules rules;

    Fixed(String id, ZoneOffset offset) {
      this.id = id;
      this.rules = ZoneRules.of(offset);
    }

    @Override
    public String getId() {
      return id;
    }

    @Override
    public ZoneRules getRules() {
      return rules;
    }
  }

  @Override
  public boolean equals(Object obj) {
    return obj instanceof ZoneId && getId().equals(((ZoneId) obj).getId());
  }

  @Override
  public int hashCode() {
    return getId().hashCode();
  }

  @Override
  public String toString() {
    return getId();
  }
}
