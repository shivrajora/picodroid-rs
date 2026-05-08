// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

public class TextView extends View {
  public TextView() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void setText(String text);

  public native void setTextColor(int argb);

  /**
   * Mirrors Android's {@code TextView.setIncludeFontPadding(boolean)}. When {@code false}, strips
   * the font's top side-bearing whitespace so the label box hugs the glyphs, balancing the visible
   * gap above and below the label inside a {@link LinearLayout}. Default {@code true}.
   */
  public native void setIncludeFontPadding(boolean include);
}
