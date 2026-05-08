// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Mirrors {@code android.view.ViewGroup}. Common parent for layout containers — owns the {@code
 * addView} / {@code removeView} / {@code getChildAt} family that every layout uses, plus the nested
 * {@link LayoutParams} that subclass {@code LinearLayout.LayoutParams} extends with weight/gravity.
 *
 * <p>Concrete subclasses (LinearLayout, FrameLayout, ScrollView, SwipeRefreshLayout, AdapterView
 * subclasses) inherit the native {@code addView} call and may add gravity / orientation /
 * adapter-specific setters of their own.
 */
public abstract class ViewGroup extends View {
  protected ViewGroup(int nativeHandle) {
    super(nativeHandle);
  }

  /** Add {@code child} to this layout, reusing whatever {@link View.LayoutParams} the child has. */
  public native void addView(View child);

  /**
   * Add {@code child} with explicit layout parameters. Records the params on the child via {@link
   * View#setLayoutParams}, applies width/height via {@link View#setSize}, and threads any
   * subclass-specific fields ({@code LinearLayout.LayoutParams.weight}) into the LVGL flex layout.
   */
  public void addView(View child, LayoutParams params) {
    if (params != null) {
      child.setLayoutParams(params);
      child.setSize(params.width, params.height);
    }
    addView(child);
    if (params instanceof picodroid.widget.LinearLayout.LayoutParams) {
      picodroid.widget.LinearLayout.LayoutParams lp =
          (picodroid.widget.LinearLayout.LayoutParams) params;
      if (lp.weight > 0) {
        child.nativeSetFlexGrow((int) lp.weight);
      }
    }
  }

  public native void removeView(View child);

  public native void removeAllViews();

  public native int getChildCount();

  /**
   * Reserved for the resource-system milestone — picodroid currently has no LVGL→Java reverse map,
   * so child-by-index lookup can't return the original {@link View} reference. {@link
   * #getChildCount} is fine to use; iterate children via the view tree you constructed instead of
   * by index for now.
   */
  public View getChildAt(int index) {
    throw new UnsupportedOperationException(
        "ViewGroup.getChildAt(int) deferred to the resource-system milestone");
  }

  /**
   * Width/height contract shared by every layout. {@link #MATCH_PARENT} maps to LVGL's 100%-of-
   * parent sizing; {@link #WRAP_CONTENT} maps to {@code LV_SIZE_CONTENT}; positive integers are
   * absolute pixels.
   */
  public static class LayoutParams {
    public static final int MATCH_PARENT = -1;
    public static final int WRAP_CONTENT = -2;

    public int width;
    public int height;

    public LayoutParams(int width, int height) {
      this.width = width;
      this.height = height;
    }

    public LayoutParams(LayoutParams source) {
      this.width = source.width;
      this.height = source.height;
    }
  }
}
