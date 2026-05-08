// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

public class Button extends View {
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

  public native void setText(String text);
}
