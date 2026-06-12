// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Standard gravity constants for placing a view within a larger container. Mirrors {@code
 * android.view.Gravity} — every constant keeps Android's exact bit value, so ported code and
 * developer muscle memory ({@code Gravity.CENTER}, {@code Gravity.END}) carry over verbatim.
 *
 * <p>Pass to {@link picodroid.widget.LinearLayout#setGravity(int)} to align children along the
 * layout's main axis. The framework decodes the Android bit layout natively (LEFT/TOP → flex
 * start, RIGHT/BOTTOM → flex end, CENTER bits → flex center); {@link #START}/{@link #END} carry
 * the same axis bits as LEFT/RIGHT, so they resolve identically on this LTR-only platform.
 */
public final class Gravity {
  private Gravity() {}

  /** Constant indicating that no gravity has been set. */
  public static final int NO_GRAVITY = 0x0000;

  /** Push object to the top of its container, not changing its size. */
  public static final int TOP = 0x30;

  /** Push object to the bottom of its container, not changing its size. */
  public static final int BOTTOM = 0x50;

  /** Push object to the left of its container, not changing its size. */
  public static final int LEFT = 0x03;

  /** Push object to the right of its container, not changing its size. */
  public static final int RIGHT = 0x05;

  /** Place object in the vertical center of its container, not changing its size. */
  public static final int CENTER_VERTICAL = 0x10;

  /** Place object in the horizontal center of its container, not changing its size. */
  public static final int CENTER_HORIZONTAL = 0x01;

  /** Place the object in the center of its container in both axes, not changing its size. */
  public static final int CENTER = CENTER_VERTICAL | CENTER_HORIZONTAL;

  /** Grow the vertical size of the object if needed so it completely fills its container. */
  public static final int FILL_VERTICAL = 0x70;

  /** Grow the horizontal size of the object if needed so it completely fills its container. */
  public static final int FILL_HORIZONTAL = 0x07;

  /** Grow the object in both axes if needed so it completely fills its container. */
  public static final int FILL = FILL_VERTICAL | FILL_HORIZONTAL;

  /**
   * Push object to the beginning of its container. Matches Android's relative-direction value;
   * picodroid is LTR-only, so this behaves exactly like {@link #LEFT}.
   */
  public static final int START = 0x00800003;

  /**
   * Push object to the end of its container. Matches Android's relative-direction value;
   * picodroid is LTR-only, so this behaves exactly like {@link #RIGHT}.
   */
  public static final int END = 0x00800005;
}
