// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

import picodroid.content.Context;

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

  /** A framework layout's handle and the {@link Context} it was made with. */
  protected ViewGroup(Context context, int nativeHandle) {
    super(context, nativeHandle);
  }

  /**
   * Mirrors {@code android.view.ViewGroup(Context)}: a group that lays its children out itself.
   * Subclass it and override {@link #onLayout} (and {@link #onMeasure} for a {@code wrap_content}
   * size), as on Android: the children are placed by {@link View#layout} inside {@code onLayout},
   * which runs on the main thread after the group is added to, resized or asked to by {@link
   * #requestLayout}. The group is 0 by 0 until a size or layout params give it one.
   */
  public ViewGroup(Context context) {
    super(context, nativeCreateGroup());
    mCustomGroup = true;
    nativeWatchSize();
  }

  /** A plain, borderless, unscrolled container with no layout of its own. */
  private static native int nativeCreateGroup();

  /** Has the renderer call {@link #fireLayout} whenever this group's size changes. */
  private native void nativeWatchSize();

  /**
   * Set by {@link #ViewGroup(Context)}: this group places its children through {@link #onLayout}.
   */
  boolean mCustomGroup;

  /** An {@link #onLayout} is queued on the main thread and has not run yet. */
  private boolean mLayoutPending;

  /** The queued task that runs {@link #onLayout}; made on the first request. */
  private Runnable mLayoutTask;

  /** The frame {@link #onLayout} last ran with, for its {@code changed} flag. */
  private int mLastLeft;

  private int mLastTop;
  private int mLastWidth = -1;
  private int mLastHeight = -1;

  /**
   * Mirrors {@code android.view.ViewGroup#onLayout}: place the children with {@link View#layout},
   * within {@code (left, top)}-{@code (right, bottom)}, this group's frame in its parent. Called
   * for a group made with {@link #ViewGroup(Context)}; the framework's layouts place their children
   * natively and never call it. The default does nothing.
   */
  protected void onLayout(boolean changed, int left, int top, int right, int bottom) {}

  @Override
  boolean layoutsItself() {
    return mCustomGroup;
  }

  /** Queues one {@link #onLayout} on the main thread; several requests before it runs cost one. */
  void scheduleLayout() {
    if (mLayoutPending || isReleased()) {
      return;
    }
    mLayoutPending = true;
    if (mLayoutTask == null) {
      mLayoutTask = new LayoutTask(this);
    }
    mainExecutor().execute(mLayoutTask);
  }

  /** Reads this group's frame and runs {@link #onLayout} with it. */
  final void performLayout() {
    mLayoutPending = false;
    if (isReleased()) {
      return;
    }
    int l = getLeft();
    int t = getTop();
    int w = getWidth();
    int h = getHeight();
    boolean changed = l != mLastLeft || t != mLastTop || w != mLastWidth || h != mLastHeight;
    mLastLeft = l;
    mLastTop = t;
    mLastWidth = w;
    mLastHeight = h;
    onLayout(changed, l, t, l + w, t + h);
  }

  @Override
  void afterLayout() {
    if (mCustomGroup) {
      performLayout();
    }
  }

  /** From the renderer: this group's size changed, so its children need placing again. */
  void fireLayout() {
    if (mCustomGroup) {
      scheduleLayout();
    }
  }

  /** The main-thread task behind {@link #requestLayout}; one per custom group, reused. */
  private static final class LayoutTask implements Runnable {
    private final ViewGroup group;

    LayoutTask(ViewGroup group) {
      this.group = group;
    }

    @Override
    public void run() {
      group.performLayout();
    }
  }

  /**
   * Mirrors {@code android.view.ViewGroup#measureChildren}: measures every child that is not {@link
   * #GONE} with {@link #measureChild}.
   */
  protected void measureChildren(int widthMeasureSpec, int heightMeasureSpec) {
    for (int i = 0; i < mChildCount; i++) {
      View child = mChildren[i];
      if (child.getVisibility() != GONE) {
        measureChild(child, widthMeasureSpec, heightMeasureSpec);
      }
    }
  }

  /**
   * Mirrors {@code android.view.ViewGroup#measureChild}: measures {@code child} with the specs its
   * {@link LayoutParams} ask for within this group's; a child without params wraps its content.
   */
  protected void measureChild(View child, int parentWidthMeasureSpec, int parentHeightMeasureSpec) {
    LayoutParams lp = child.getLayoutParams();
    int width = lp == null ? LayoutParams.WRAP_CONTENT : lp.width;
    int height = lp == null ? LayoutParams.WRAP_CONTENT : lp.height;
    child.measure(
        getChildMeasureSpec(parentWidthMeasureSpec, 0, width),
        getChildMeasureSpec(parentHeightMeasureSpec, 0, height));
  }

  /**
   * Mirrors {@code android.view.ViewGroup#getChildMeasureSpec}: the spec a child with layout
   * dimension {@code childDimension} gets from a parent measured with {@code spec}, less {@code
   * padding}.
   */
  public static int getChildMeasureSpec(int spec, int padding, int childDimension) {
    int specMode = MeasureSpec.getMode(spec);
    int size = Math.max(0, MeasureSpec.getSize(spec) - padding);
    int resultSize = 0;
    int resultMode = MeasureSpec.UNSPECIFIED;
    if (childDimension >= 0) {
      resultSize = childDimension;
      resultMode = MeasureSpec.EXACTLY;
    } else if (specMode == MeasureSpec.EXACTLY || specMode == MeasureSpec.AT_MOST) {
      if (childDimension == LayoutParams.MATCH_PARENT) {
        resultSize = size;
        resultMode = specMode == MeasureSpec.EXACTLY ? MeasureSpec.EXACTLY : MeasureSpec.AT_MOST;
      } else {
        resultSize = size;
        resultMode = MeasureSpec.AT_MOST;
      }
    }
    return MeasureSpec.makeMeasureSpec(resultSize, resultMode);
  }

  /** Add {@code child} to this layout, reusing whatever {@link View.LayoutParams} the child has. */
  /**
   * The children in {@link #addView} order, as {@code android.view.ViewGroup} keeps them; what
   * {@link #getChildAt} indexes. Allocated on the first add, doubled as needed.
   */
  private View[] mChildren;

  private int mChildCount;

  /**
   * Adds {@code child} to this layout with the {@link LayoutParams} it already carries (an inflated
   * view's, or what {@link View#setLayoutParams} set), as on Android; a child without any keeps the
   * size and position it was given directly. A container or a view that draws itself, never sized,
   * is the exception: Android gives a child without params this group's {@link
   * #generateDefaultLayoutParams}, and one left at the renderer's default size (160×160) is never
   * what the app meant (QA 2026-10-22, U4: a fragment's {@code FrameLayout} in a pager; a custom
   * view whose {@code onMeasure} never ran, 2026-10-26).
   */
  public void addView(View child) {
    LayoutParams params = child == null ? null : child.getLayoutParams();
    if (params == null && (child instanceof ViewGroup || child.drawsItself()) && !child.mSized) {
      params = generateDefaultLayoutParams();
    }
    if (params != null) {
      addView(child, params);
    } else {
      attach(child);
    }
  }

  /**
   * Mirrors {@code android.view.ViewGroup#generateDefaultLayoutParams}: the params a child added
   * without any gets, {@code wrap_content} on both axes here; {@link picodroid.widget.FrameLayout}
   * and {@link picodroid.widget.LinearLayout} override it as Android's do. picodroid applies it
   * only to a {@code ViewGroup} child that was never sized ({@link #addView(View)}).
   */
  protected LayoutParams generateDefaultLayoutParams() {
    return new LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT);
  }

  private void attach(View child) {
    checkNotReleased(child);
    checkNotAncestor(child);
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
    link(child);
  }

  /**
   * The inflater's add for a child its native path made: the widget is already under this group's
   * and laid out as {@code params} say, so only the Java side joins. No parent to leave, nothing to
   * check: the child is new.
   */
  final void attachInflated(View child, LayoutParams params) {
    child.layoutParams = params;
    child.mSized = true;
    // link(), spelled out: this runs once per inflated view, and a call is not free here.
    if (mChildren == null) {
      mChildren = new View[4];
    } else if (mChildCount == mChildren.length) {
      View[] bigger = new View[mChildren.length * 2];
      System.arraycopy(mChildren, 0, bigger, 0, mChildCount);
      mChildren = bigger;
    }
    mChildren[mChildCount++] = child;
    child.mParent = this;
    onViewAdded(child);
  }

  /** Puts {@code child} in the child list, as the last one, and says so. */
  private void link(View child) {
    if (mChildren == null) {
      mChildren = new View[4];
    } else if (mChildCount == mChildren.length) {
      View[] bigger = new View[mChildren.length * 2];
      System.arraycopy(mChildren, 0, bigger, 0, mChildCount);
      mChildren = bigger;
    }
    mChildren[mChildCount++] = child;
    child.mParent = this;
    onViewAdded(child);
    if (mCustomGroup) {
      scheduleLayout();
    }
  }

  /**
   * Called after {@code child} joined this group through any {@link #addView} overload (the
   * inflater's included). Mirrors {@code android.view.ViewGroup#onViewAdded}; the default does
   * nothing.
   */
  protected void onViewAdded(View child) {}

  private native void nativeAddView(View child);

  /**
   * A {@code RelativeLayout} child's id and rules, for {@link picodroid.widget.RelativeLayout}:
   * {@link View}'s layout natives are package-private, and a subclass in another package reaches
   * this one instead.
   */
  protected static void setRelativeRules(View child, int id, int flags, int[] anchors) {
    child.nativeSetRelativeRules(id, flags, anchors);
  }

  /**
   * Adds {@code child} at {@code index} among this group's children, as on Android; {@code -1} (or
   * any index past the end) appends. The child's own {@link LayoutParams}, if any, apply.
   */
  public void addView(View child, int index) {
    addView(child, index, child == null ? null : child.getLayoutParams());
  }

  /** Adds {@code child} at {@code index} with {@code params}; see {@link #addView(View, int)}. */
  public void addView(View child, int index, LayoutParams params) {
    // A child of this group moved to the end still needs the widget moved: attach() re-parents
    // it to the parent it already has, which LVGL treats as a no-op.
    boolean moved = child != null && child.mParent == this;
    addView(child, params);
    if (child != null && index >= 0 && (index < mChildCount - 1 || moved)) {
      int at = Math.min(index, mChildCount - 1);
      // The child sits last in mChildren (attach appends): slide it down to index.
      System.arraycopy(mChildren, at, mChildren, at + 1, mChildCount - 1 - at);
      mChildren[at] = child;
      nativeMoveChildTo(child, at);
    }
  }

  private native void nativeMoveChildTo(View child, int index);

  /**
   * Mirrors {@code android.view.ViewGroup#detachViewFromParent(View)}: takes {@code child} out of
   * this group's child list without freeing it, for a later {@link #attachViewToParent}. The widget
   * stays where it is; this is the bookkeeping half of a row moving within a recycling {@code
   * ListView}. A view that is not a child is left alone.
   */
  protected void detachViewFromParent(View child) {
    if (child != null) {
      detachChild(child);
    }
  }

  /**
   * Mirrors {@code android.view.ViewGroup#attachViewToParent}: puts a view {@link
   * #detachViewFromParent} took out back into this group's child list at {@code index} ({@code -1}
   * or past the end appends) and moves its widget to that index among this group's. The widget must
   * still be under this group's: unlike Android, picodroid never detaches it. {@code params}, when
   * given, replace the child's layout params without re-applying them.
   */
  protected void attachViewToParent(View child, int index, LayoutParams params) {
    checkNotReleased(child);
    if (child.mParent != null) {
      throw new IllegalStateException("attachViewToParent: the view still has a parent");
    }
    if (params != null) {
      child.layoutParams = params;
    }
    if (mChildren == null) {
      mChildren = new View[4];
    } else if (mChildCount == mChildren.length) {
      View[] bigger = new View[mChildren.length * 2];
      System.arraycopy(mChildren, 0, bigger, 0, mChildCount);
      mChildren = bigger;
    }
    int at = index < 0 || index > mChildCount ? mChildCount : index;
    System.arraycopy(mChildren, at, mChildren, at + 1, mChildCount - at);
    mChildren[at] = child;
    mChildCount++;
    child.mParent = this;
    nativeMoveChildTo(child, at);
  }

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
   * A view cannot be added to itself or to one of its own descendants: the widget tree would have a
   * cycle, which layout, draw and delete walk forever. Android reports this as "the specified child
   * already has a parent"; picodroid lets {@link #addView} move a parented view, so the cycle is
   * named directly. The native side checks the same thing on the widget tree.
   */
  private void checkNotAncestor(View child) {
    for (ViewGroup p = this; p != null; p = p.mParent) {
      if (p == child) {
        throw new IllegalStateException(
            "addView: the child is this view or one of its ancestors; a view cannot contain"
                + " itself");
      }
    }
  }

  /**
   * Add {@code child} with explicit layout parameters. Records the params on the child via {@link
   * View#setLayoutParams}, applies width/height via {@link View#setSize}, and threads any
   * subclass-specific fields ({@code LinearLayout.LayoutParams.weight} and {@code gravity}) into
   * the layout.
   */
  public void addView(View child, LayoutParams params) {
    if (params != null) {
      child.layoutParams = params;
    }
    attach(child);
    if (params != null) {
      applyLayoutParams(child, params, false);
    }
  }

  /**
   * Lays {@code child} out by {@code params}: its size, then what the params' class carries (weight
   * and gravity, margins). {@code again} is {@link View#setLayoutParams} on a child already placed:
   * a field now zero must clear what earlier params set, where a first add skips it.
   */
  final void applyLayoutParams(View child, LayoutParams params, boolean again) {
    child.applyLayoutSize(params.width, params.height);
    if (again) {
      // Params of a plainer class carry none of these: Android's parent converts them and the
      // child is left with no weight, gravity or margin. Clear what earlier params set.
      if (!(params instanceof picodroid.widget.LinearLayout.LayoutParams)) {
        child.nativeSetWeight(0);
        child.nativeSetLayoutGravity(0);
      }
      if (!(params instanceof MarginLayoutParams)) {
        child.nativeSetMargins(0, 0, 0, 0);
      }
      if (!(params instanceof picodroid.widget.FrameLayout.LayoutParams)
          && this instanceof picodroid.widget.FrameLayout) {
        child.nativeSetFrameGravity(0, 0, 0);
      }
    }
    if (params instanceof picodroid.widget.LinearLayout.LayoutParams) {
      picodroid.widget.LinearLayout.LayoutParams lp =
          (picodroid.widget.LinearLayout.LayoutParams) params;
      if (lp.weight > 0) {
        // Thousandths: fractional ratios survive the integer (1.5f : 1f -> 1500 : 1000).
        child.nativeSetWeight(Math.max(1, Math.round(lp.weight * 1000)));
      } else if (again) {
        child.nativeSetWeight(0);
      }
      if (lp.gravity != 0 || again) {
        child.nativeSetLayoutGravity(lp.gravity);
      }
    }
    if (params instanceof picodroid.widget.FrameLayout.LayoutParams) {
      // A FrameLayout places a child by its gravity and margins, as Android's does: from the
      // corner (or centre) the gravity names, moved in by the margins on that side.
      picodroid.widget.FrameLayout.LayoutParams lp =
          (picodroid.widget.FrameLayout.LayoutParams) params;
      int gravity = lp.gravity;
      if (again
          || (gravity | lp.leftMargin | lp.topMargin | lp.rightMargin | lp.bottomMargin) != 0) {
        int horizontal = gravity & Gravity.HORIZONTAL_GRAVITY_MASK;
        int vertical = gravity & Gravity.VERTICAL_GRAVITY_MASK;
        int dx =
            horizontal == Gravity.RIGHT
                ? -lp.rightMargin
                : (horizontal == Gravity.CENTER_HORIZONTAL
                    ? lp.leftMargin - lp.rightMargin
                    : lp.leftMargin);
        int dy =
            vertical == Gravity.BOTTOM
                ? -lp.bottomMargin
                : (vertical == Gravity.CENTER_VERTICAL
                    ? lp.topMargin - lp.bottomMargin
                    : lp.topMargin);
        child.nativeSetFrameGravity(gravity, dx, dy);
      }
    } else if (params instanceof MarginLayoutParams) {
      MarginLayoutParams lp = (MarginLayoutParams) params;
      if (again || (lp.leftMargin | lp.topMargin | lp.rightMargin | lp.bottomMargin) != 0) {
        child.nativeSetMargins(lp.leftMargin, lp.topMargin, lp.rightMargin, lp.bottomMargin);
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
      if (mCustomGroup) {
        scheduleLayout();
      }
    }
    // Not a child (or already released): a no-op, as on Android.
  }

  /** Mirrors Android: removes the child at {@code index}; out of range is a no-op. */
  public void removeViewAt(int index) {
    removeView(getChildAt(index));
  }

  /** Mirrors Android: the index of {@code child} among this group's children, or -1. */
  public int indexOfChild(View child) {
    for (int i = 0; i < mChildCount; i++) {
      if (mChildren[i] == child) {
        return i;
      }
    }
    return -1;
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

  /** The children added through {@link #addView}, as {@link #getChildAt} indexes them. */
  public int getChildCount() {
    return mChildCount;
  }

  /**
   * The widget's own child count. It can differ from {@link #getChildCount()}: a widget may own
   * children of its own (a SwipeRefreshLayout's spinner), and a ListView's rows are all widgets.
   */
  protected native int nativeGetChildCount();

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

  /**
   * Mirrors {@code android.view.ViewGroup.MarginLayoutParams}: layout parameters with space around
   * the child, in pixels. A {@link picodroid.widget.LinearLayout} keeps the margins clear between
   * its children; a {@link picodroid.widget.FrameLayout} places the child that far in from the edge
   * its gravity names. Margins are read when the child is added: change them before {@link
   * ViewGroup#addView}.
   */
  public static class MarginLayoutParams extends LayoutParams {
    public int leftMargin;
    public int topMargin;
    public int rightMargin;
    public int bottomMargin;

    public MarginLayoutParams(int width, int height) {
      super(width, height);
    }

    public MarginLayoutParams(LayoutParams source) {
      super(source);
    }

    public MarginLayoutParams(MarginLayoutParams source) {
      super(source);
      this.leftMargin = source.leftMargin;
      this.topMargin = source.topMargin;
      this.rightMargin = source.rightMargin;
      this.bottomMargin = source.bottomMargin;
    }

    public void setMargins(int left, int top, int right, int bottom) {
      leftMargin = left;
      topMargin = top;
      rightMargin = right;
      bottomMargin = bottom;
    }
  }
}
