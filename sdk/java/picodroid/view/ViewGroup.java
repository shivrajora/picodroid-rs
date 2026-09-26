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
public abstract class ViewGroup extends View implements ViewParent {
  protected ViewGroup(int nativeHandle) {
    super(nativeHandle);
  }

  /** Add {@code child} to this layout, reusing whatever {@link View.LayoutParams} the child has. */
  /**
   * The children in {@link #addView} order, as {@code android.view.ViewGroup} keeps them; what
   * {@link #getChildAt} indexes. Allocated on the first add, doubled as needed.
   */
  private View[] mChildren;

  private int mChildCount;

  public void addView(View child) {
    checkNotReleased(child);
    nativeAddView(child);
    if (child == null) {
      return;
    }
    ViewGroup previous = child.mParent;
    if (previous != null) {
      // Android throws here ("The specified child already has a parent"). picodroid moves the
      // view instead: removeView frees a view, so a move is the only way to reparent one. The old
      // parent's list must let go of it, or that entry keeps the subtree alive.
      previous.detachChild(child);
    }
    if (mChildren == null) {
      mChildren = new View[4];
    } else if (mChildCount == mChildren.length) {
      View[] bigger = new View[mChildren.length * 2];
      System.arraycopy(mChildren, 0, bigger, 0, mChildCount);
      mChildren = bigger;
    }
    mChildren[mChildCount++] = child;
    child.mParent = this;
  }

  private native void nativeAddView(View child);

  /**
   * Drops {@code child} from this group's list without touching its widget, and clears its parent.
   * Returns whether it was a child. {@link #removeView} and {@link View#close} free the widget
   * next; {@link #addView} moving a view to another parent does not.
   */
  boolean detachChild(View child) {
    for (int i = 0; i < mChildCount; i++) {
      if (mChildren[i] == child) {
        System.arraycopy(mChildren, i + 1, mChildren, i, mChildCount - i - 1);
        mChildren[--mChildCount] = null;
        child.mParent = null;
        return true;
      }
    }
    return false;
  }

  /**
   * A view {@link #removeView} released has no widget left to add: refuse it here, on the Java
   * side, before any native code sees the dead handle.
   */
  private static void checkNotReleased(View child) {
    if (child.isReleased()) {
      throw new IllegalStateException(
          "addView: this view was released by removeView; picodroid frees a removed view, create a"
              + " new one");
    }
  }

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
        // Weights are relative, so a fixed x10 scale preserves fractional
        // ratios (1.5f : 1f -> 15 : 10) that a plain int cast would destroy.
        // LVGL's flex-grow is u8, capping effective weights at 25.5.
        child.nativeSetFlexGrow(Math.max(1, Math.round(lp.weight * 10)));
      }
    }
  }

  /**
   * Detaches {@code child}. Unlike Android, picodroid also frees the child's widget here — an
   * embedded panel cannot afford detached trees waiting for a re-add — so a removed view cannot be
   * added again: {@link #addView} throws {@code IllegalStateException} for it. Build a fresh view,
   * or hide one with {@link View#setVisibility} when it will come back. {@link View#close} on a
   * child is this call from the child's side.
   */
  public void removeView(View child) {
    if (child != null && detachChild(child)) {
      nativeRemoveView(child);
      child.release();
    }
    // Not a child (or already released): a no-op, as on Android.
  }

  private native void nativeRemoveView(View child);

  public void removeAllViews() {
    nativeRemoveAllViews();
    for (int i = 0; i < mChildCount; i++) {
      mChildren[i].release();
      mChildren[i] = null;
    }
    mChildCount = 0;
  }

  /** Releases this group's children with it: their widgets went with the group's. */
  @Override
  void release() {
    for (int i = 0; i < mChildCount; i++) {
      mChildren[i].release();
      mChildren[i] = null;
    }
    mChildCount = 0;
    super.release();
  }

  private native void nativeRemoveAllViews();

  public native int getChildCount();

  /**
   * The child at {@code index} in {@link #addView} order, or {@code null} when there is none, as on
   * Android.
   */
  public View getChildAt(int index) {
    if (index < 0 || index >= mChildCount) {
      return null;
    }
    return mChildren[index];
  }

  /**
   * Width/height contract shared by every layout. {@link #MATCH_PARENT} maps to LVGL's 100%-of-
   * parent sizing; {@link #WRAP_CONTENT} maps to {@code LV_SIZE_CONTENT}; positive integers are
   * absolute pixels.
   */
  @Override
  protected View findViewTraversal(int id) {
    if (id == getId()) {
      return this;
    }
    for (int i = 0; i < mChildCount; i++) {
      View found = mChildren[i].findViewTraversal(id);
      if (found != null) {
        return found;
      }
    }
    return null;
  }

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
