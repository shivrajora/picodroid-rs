// SPDX-License-Identifier: GPL-3.0-only
package picodroid.text;

/**
 * Text helpers mirroring {@code android.text.TextUtils}: the {@link TruncateAt} kinds that {@link
 * picodroid.widget.TextView#setEllipsize} takes, and {@link #isEmpty}.
 */
public final class TextUtils {

  private TextUtils() {}

  /**
   * Where {@link picodroid.widget.TextView#setEllipsize} cuts text that does not fit. Mirrors
   * {@code android.text.TextUtils.TruncateAt}. Here {@code START} and {@code MIDDLE} render like
   * {@code END} — three ASCII dots at the end of the last line that fits — and {@code MARQUEE}
   * scrolls the text circularly whenever it is wider than the view.
   */
  public enum TruncateAt {
    START,
    MIDDLE,
    END,
    MARQUEE
  }

  /** Mirrors Android: {@code true} for {@code null} or a zero-length sequence. */
  public static boolean isEmpty(CharSequence s) {
    return s == null || s.length() == 0;
  }
}
