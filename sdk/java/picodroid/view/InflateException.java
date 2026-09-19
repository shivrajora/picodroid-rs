// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Mirrors {@code android.view.InflateException}: thrown by {@link LayoutInflater} for a layout this
 * framework cannot build.
 */
public class InflateException extends RuntimeException {
  public InflateException(String message) {
    super(message);
  }
}
