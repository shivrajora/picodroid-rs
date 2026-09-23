// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.res;

/**
 * A single colour standing in for {@code android.content.res.ColorStateList}. Android's class maps
 * view states (pressed, disabled, ...) to colours; picodroid models no state sets, so every state
 * resolves to the one colour given to {@link #valueOf(int)}. The class exists so that tint setters
 * keep Android's signatures ({@code setProgressTintList(ColorStateList)}) and idioms ({@code
 * ColorStateList.valueOf(Color.RED)}).
 */
public class ColorStateList {
  private final int color;

  private ColorStateList(int color) {
    this.color = color;
  }

  /** A list whose every state is {@code color} (ARGB). */
  public static ColorStateList valueOf(int color) {
    return new ColorStateList(color);
  }

  /** The colour for the default state; here, the only colour. */
  public int getDefaultColor() {
    return color;
  }

  /** Always {@code false}: a single colour does not vary with state. */
  public boolean isStateful() {
    return false;
  }

  /** Every state resolves to the list's one colour, so {@code defaultColor} is never needed. */
  public int getColorForState(int[] stateSet, int defaultColor) {
    return color;
  }

  /** The same colour with its alpha replaced by {@code alpha} (0..255). */
  public ColorStateList withAlpha(int alpha) {
    return new ColorStateList((color & 0x00FFFFFF) | ((alpha & 0xFF) << 24));
  }
}
