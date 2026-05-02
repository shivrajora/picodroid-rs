// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

public class ListView extends View {
  public ListView() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void addItem(String text);
}
