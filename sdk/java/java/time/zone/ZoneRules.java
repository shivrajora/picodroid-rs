// SPDX-License-Identifier: GPL-3.0-only
package java.time.zone;

import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneOffset;

/**
 * The offset rules of a zone. Every zone here has one fixed offset (there is no tz database), so
 * {@link #isFixedOffset()} is always true and both {@code getOffset} forms return the same value.
 */
public final class ZoneRules {
  private final ZoneOffset offset;

  private ZoneRules(ZoneOffset offset) {
    this.offset = offset;
  }

  public static ZoneRules of(ZoneOffset offset) {
    if (offset == null) {
      throw new NullPointerException("offset");
    }
    return new ZoneRules(offset);
  }

  public boolean isFixedOffset() {
    return true;
  }

  public ZoneOffset getOffset(Instant instant) {
    return offset;
  }

  public ZoneOffset getOffset(LocalDateTime localDateTime) {
    return offset;
  }

  public boolean isDaylightSavings(Instant instant) {
    return false;
  }

  @Override
  public boolean equals(Object obj) {
    return obj instanceof ZoneRules && ((ZoneRules) obj).offset.equals(offset);
  }

  @Override
  public int hashCode() {
    return offset.hashCode();
  }

  @Override
  public String toString() {
    return "ZoneRules[" + offset + "]";
  }
}
