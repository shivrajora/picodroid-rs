// SPDX-License-Identifier: GPL-3.0-only
package java.util;

/**
 * The {@code java.util.Objects} subset apps reach for first. A real class with bytecode, not a
 * native stub: {@link #equals}, {@link #hashCode} and {@link #toString} must virtual-dispatch to
 * the argument's own overrides, which only interpreted code can do.
 */
public final class Objects {
  private Objects() {}

  public static boolean equals(Object a, Object b) {
    return (a == b) || (a != null && a.equals(b));
  }

  public static int hashCode(Object o) {
    return o != null ? o.hashCode() : 0;
  }

  public static int hash(Object... values) {
    if (values == null) {
      return 0;
    }
    int result = 1;
    for (Object element : values) {
      result = 31 * result + (element == null ? 0 : element.hashCode());
    }
    return result;
  }

  public static String toString(Object o) {
    return String.valueOf(o);
  }

  public static String toString(Object o, String nullDefault) {
    return (o != null) ? o.toString() : nullDefault;
  }

  public static <T> T requireNonNull(T obj) {
    if (obj == null) {
      throw new NullPointerException();
    }
    return obj;
  }

  public static <T> T requireNonNull(T obj, String message) {
    if (obj == null) {
      throw new NullPointerException(message);
    }
    return obj;
  }

  public static boolean isNull(Object obj) {
    return obj == null;
  }

  public static boolean nonNull(Object obj) {
    return obj != null;
  }
}
