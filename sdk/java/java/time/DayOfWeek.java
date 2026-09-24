// SPDX-License-Identifier: GPL-3.0-only
package java.time;

/**
 * A day of the week, Monday (1) to Sunday (7), as in the JDK. {@code getDisplayName} is not served:
 * there is no {@code Locale} or {@code TextStyle} here; {@code DateTimeFormatter}'s {@code EEE} /
 * {@code EEEE} letters print the English names.
 */
public enum DayOfWeek {
  MONDAY,
  TUESDAY,
  WEDNESDAY,
  THURSDAY,
  FRIDAY,
  SATURDAY,
  SUNDAY;

  private static final DayOfWeek[] ENUMS = DayOfWeek.values();

  /** Monday is 1, Sunday is 7. */
  public static DayOfWeek of(int dayOfWeek) {
    if (dayOfWeek < 1 || dayOfWeek > 7) {
      throw new DateTimeException("Invalid value for DayOfWeek: " + dayOfWeek);
    }
    return ENUMS[dayOfWeek - 1];
  }

  /** Monday is 1, Sunday is 7. */
  public int getValue() {
    return ordinal() + 1;
  }

  public DayOfWeek plus(long days) {
    int amount = (int) (days % 7);
    return ENUMS[(ordinal() + (amount + 7)) % 7];
  }

  public DayOfWeek minus(long days) {
    return plus(-(days % 7));
  }
}
