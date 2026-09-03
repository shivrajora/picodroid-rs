// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

/**
 * Push button. Mirrors {@code android.widget.Button}, a {@link TextView}. The native object is a
 * button with a child label, so {@link #setText} is re-declared here and routed to that label;
 * {@link #setTextColor} is inherited unchanged (the colour style cascades to the label) and {@link
 * #setIncludeFontPadding} pads the button box rather than the label.
 */
public class Button extends TextView {
  public Button(String text) {
    super(nativeCreate(text));
  }

  public Button(Context ctx, String text) {
    super(nativeCreate(text));
  }

  public Button(Context ctx) {
    super(nativeCreate(""));
  }

  private static native int nativeCreate(String text);

  @Override
  public native void setText(String text);

  /** Re-declared like {@link #setText}: reads the child label, not the button box. */
  @Override
  public native CharSequence getText();
}
