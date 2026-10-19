// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.ViewGroup;

public class ScrollView extends ViewGroup {

  public ScrollView() {
    super(nativeCreate());
  }

  public ScrollView(Context ctx) {
    super(ctx, nativeCreate());
  }

  /** For the inflater's native path, which makes the widget itself ({@link InflaterSupport}). */
  protected ScrollView(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();
}
