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
}
