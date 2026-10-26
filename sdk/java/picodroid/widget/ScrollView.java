// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.ViewGroup;

public class ScrollView extends ViewGroup {

  public ScrollView(Context ctx) {
    super(ctx, nativeCreate());
  }

  /** For the inflater's native path, which makes the widget itself ({@link InflaterSupport}). */
  protected ScrollView(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  /**
   * The params a child added without any gets: {@code match_parent} wide, {@code wrap_content}
   * tall. Android's {@code ScrollView} hands out its {@code FrameLayout} parent's {@code
   * match_parent} on both axes and then measures the child's height unconstrained; picodroid has no
   * measure pass, and a {@code match_parent} height here would be the viewport's, leaving nothing
   * to scroll.
   */
  @Override
  protected ViewGroup.LayoutParams generateDefaultLayoutParams() {
    return new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT);
  }
}
