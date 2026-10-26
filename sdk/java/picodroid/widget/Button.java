// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

/**
 * Push button. Mirrors {@code android.widget.Button}, a {@link TextView}. The native object is a
 * button with a child label: {@link #setText}, {@link #setTextSize}, {@link #setIncludeFontPadding}
 * and the line-mode setters ({@link #setSingleLine}, {@link #setEllipsize}, {@link #setMaxLines})
 * reach that label, {@link #setTextColor} cascades to it, and a content-sized button grows with its
 * face.
 */
public class Button extends TextView {
  public Button(Context ctx) {
    super(ctx, nativeCreate(""));
    mText = "";
    // Focusable by default, as on Android: a stop for the keys without a `setFocusable(true)`
    // of its own, and `requestFocus()` works. LVGL walks a button either way; this is what
    // gives it the focus border a keys board draws (app-portability A7, F5).
    setFocusable(true);
  }

  /**
   * For the inflater's native path, which makes the button itself ({@link InflaterSupport}) and
   * writes its text and focusability, as the constructor above would.
   */
  protected Button(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate(String text);
}
