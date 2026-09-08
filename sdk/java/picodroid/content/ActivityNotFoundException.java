// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

/**
 * Thrown by {@code startActivity} when the Intent names a package that is not installed on this
 * device (or a class this app does not have). Mirrors {@code
 * android.content.ActivityNotFoundException}.
 */
public class ActivityNotFoundException extends RuntimeException {
  public ActivityNotFoundException() {}

  public ActivityNotFoundException(String message) {
    super(message);
  }
}
