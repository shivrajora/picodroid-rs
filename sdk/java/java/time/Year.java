// SPDX-License-Identifier: GPL-3.0-only
package java.time;

/**
 * A year in the ISO calendar. Only the parts a date needs are served: {@link #isLeap(long)}, {@link
 * #of}, {@link #getValue}, {@link #isLeap()} and {@link #length()}.
 */
public final class Year implements Comparable<Year> {
  public static final int MIN_VALUE = -999_999_999;
  public static final int MAX_VALUE = 999_999_999;

  private final int year;

  private Year(int year) {
    this.year = year;
  }

  public static Year of(int isoYear) {
    if (isoYear < MIN_VALUE || isoYear > MAX_VALUE) {
      throw new DateTimeException("Invalid value for Year: " + isoYear);
    }
    return new Year(isoYear);
  }

  /** The proleptic ISO rule: divisible by 4, except centuries not divisible by 400. */
  public static boolean isLeap(long year) {
    return (year & 3) == 0 && (year % 100 != 0 || year % 400 == 0);
  }

  public int getValue() {
    return year;
  }

  public boolean isLeap() {
    return isLeap(year);
  }

  /** 365 or 366. */
  public int length() {
    return isLeap(year) ? 366 : 365;
  }

  public LocalDate atDay(int dayOfYear) {
    return LocalDate.ofYearDay(year, dayOfYear);
  }

  @Override
  public int compareTo(Year other) {
    return year - other.year;
  }

  @Override
  public boolean equals(Object obj) {
    return obj instanceof Year && ((Year) obj).year == year;
  }

  @Override
  public int hashCode() {
    return year;
  }

  @Override
  public String toString() {
    return Integer.toString(year);
  }
}
