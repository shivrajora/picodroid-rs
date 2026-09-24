// SPDX-License-Identifier: GPL-3.0-only
package java.time;

/**
 * A month of the year, January (1) to December (12), as in the JDK. {@code getDisplayName} is not
 * served (no {@code Locale} or {@code TextStyle}); {@code DateTimeFormatter}'s {@code MMM} / {@code
 * MMMM} letters print the English names.
 */
public enum Month {
  JANUARY,
  FEBRUARY,
  MARCH,
  APRIL,
  MAY,
  JUNE,
  JULY,
  AUGUST,
  SEPTEMBER,
  OCTOBER,
  NOVEMBER,
  DECEMBER;

  private static final Month[] ENUMS = Month.values();

  /** January is 1, December is 12. */
  public static Month of(int month) {
    if (month < 1 || month > 12) {
      throw new DateTimeException("Invalid value for MonthOfYear: " + month);
    }
    return ENUMS[month - 1];
  }

  /** January is 1, December is 12. */
  public int getValue() {
    return ordinal() + 1;
  }

  public Month plus(long months) {
    int amount = (int) (months % 12);
    return ENUMS[(ordinal() + (amount + 12)) % 12];
  }

  public Month minus(long months) {
    return plus(-(months % 12));
  }

  /** Days in this month, given whether the year is a leap year. */
  public int length(boolean leapYear) {
    int m = ordinal() + 1;
    if (m == 2) {
      return leapYear ? 29 : 28;
    }
    if (m == 4 || m == 6 || m == 9 || m == 11) {
      return 30;
    }
    return 31;
  }

  public int minLength() {
    return length(false);
  }

  public int maxLength() {
    return length(true);
  }

  /** The day-of-year (1-based) of the first day of this month. */
  public int firstDayOfYear(boolean leapYear) {
    int leap = leapYear ? 1 : 0;
    int m = ordinal() + 1;
    switch (m) {
      case 1:
        return 1;
      case 2:
        return 32;
      case 3:
        return 60 + leap;
      case 4:
        return 91 + leap;
      case 5:
        return 121 + leap;
      case 6:
        return 152 + leap;
      case 7:
        return 182 + leap;
      case 8:
        return 213 + leap;
      case 9:
        return 244 + leap;
      case 10:
        return 274 + leap;
      case 11:
        return 305 + leap;
      default:
        return 335 + leap;
    }
  }

  /** The first month of the quarter this month is in. */
  public Month firstMonthOfQuarter() {
    return ENUMS[(ordinal() / 3) * 3];
  }
}
