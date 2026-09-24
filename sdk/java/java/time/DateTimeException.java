// SPDX-License-Identifier: GPL-3.0-only
package java.time;

/** Thrown for an out-of-range field or an unsupported time-zone. Mirrors the JDK class. */
public class DateTimeException extends RuntimeException {
  public DateTimeException(String message) {
    super(message);
  }

  public DateTimeException(String message, Throwable cause) {
    super(message, cause);
  }
}
