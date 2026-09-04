// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

public class TextView extends View {
  public TextView() {
    super(nativeCreate());
  }

  public TextView(Context ctx) {
    super(nativeCreate());
  }

  /**
   * For subclasses whose native object is not a bare label ({@link Button}: a button with a child
   * label) — they create their own object and hand the handle up.
   */
  protected TextView(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  public native void setText(String text);

  /**
   * Mirrors Android's {@code TextView.getText()}: the label's current text as a {@link
   * CharSequence} (a {@link String} at runtime), so the {@code getText().toString()} idiom works
   * unchanged. Returns an empty string for a label with no text.
   */
  public native CharSequence getText();

  public native void setTextColor(int argb);

  /**
   * Mirrors Android's {@code TextView.setIncludeFontPadding(boolean)}. When {@code false}, strips
   * the font's top side-bearing whitespace so the label box hugs the glyphs, balancing the visible
   * gap above and below the label inside a {@link LinearLayout}. Default {@code true}.
   */
  public native void setIncludeFontPadding(boolean include);
}
