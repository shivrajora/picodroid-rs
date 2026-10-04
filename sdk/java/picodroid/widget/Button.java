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
  public Button(String text) {
    super(nativeCreate(text));
    mText = text == null ? "" : text;
  }

  public Button(Context ctx, String text) {
    this(text);
  }

  public Button(Context ctx) {
    this("");
  }

  private static native int nativeCreate(String text);
}
