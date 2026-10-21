// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

/**
 * Mirrors {@code android.widget.ListView}. Rows come from the bound {@link Adapter}'s {@link
 * Adapter#getView getView} and are kept as direct children in position order. As on Android, only a
 * window of rows is alive at a time: the rows on screen plus one beyond each edge. A row that
 * scrolls out of the window is handed back to {@code getView} as {@code convertView} for the
 * position scrolling in, so a sixty-row or a thousand-row list costs the same as a dozen rows. The
 * list's own top and bottom padding stand in for the rows outside the window, so a row for position
 * {@code p} always sits at {@code p × rowHeight} whatever the window holds.
 *
 * <p>A list of {@link #MIN_WINDOW} rows or fewer is built whole and never recycles; {@code
 * notifyDataSetChanged()} offers each live row back as {@code convertView} for its own position,
 * and a position past the new count frees its row.
 *
 * <p>Rows share one height, that of the first row the adapter builds; Android lets each row choose
 * its own. Each row is made clickable and keypad-focusable and joins the Activity's focus group, so
 * a tap or ENTER on it fires {@link AdapterView.OnItemClickListener#onItemClick onItemClick} with
 * the row as {@code view}. Rows are stretched to the list's width, as Android's are.
 */
public class ListView extends AdapterView<Adapter> {
  /**
   * Lists this long or shorter are built whole: no measuring, no scroll events, no padding. Every
   * menu-sized list stays exactly as cheap as it was before rows recycled.
   */
  private static final int MIN_WINDOW = 8;

  /** Rows kept alive beyond each edge of the viewport, so a key press or a drag has one ready. */
  private static final int OVERSCAN = 1;

  /** The adapter position of child 0. */
  private int mFirst;

  /** One row's height in pixels, the pitch of every row; 0 until the first row is measured. */
  private int mPitch;

  /** Whether the widget reports its scroll and size changes to {@link #fireScroll}. */
  private boolean mWatching;

  public ListView() {
    super(nativeCreate());
  }

  public ListView(Context ctx) {
    super(ctx, nativeCreate());
  }

  private static native int nativeCreate();

  /**
   * Append a single native text row. Convenience kept for parity with the pre-adapter API; a list
   * with an adapter replaces these rows at its next refresh.
   */
  public native void addItem(String text);

  @Override
  protected void registerNativeItemClick() {
    nativeRegisterItemClickListener();
  }

  private native void nativeRegisterItemClickListener();

  /**
   * Invoked by the framework event loop when a row is activated (ENTER on the focused row, or a
   * touch tap). {@code slot} is the row's index among the list's children; the position is that
   * plus the window's first position. Resolves the row's stable {@code id} from the bound {@link
   * Adapter} and delivers the full Android {@code onItemClick(parent, view, position, id)}
   * callback; {@code view} is the row the adapter built, or {@code null} for an {@link #addItem}
   * row.
   */
  void fireItemClick(int slot) {
    if (onItemClickListener != null) {
      int position = mFirst + slot;
      long id = adapter != null ? adapter.getItemId(position) : position;
      View row = slot >= 0 && slot < liveRows() ? getChildAt(slot) : null;
      onItemClickListener.onItemClick(this, row, position, id);
    }
  }

  /**
   * Invoked by the framework event loop after the list scrolled or changed size: slides the window
   * of live rows to cover the viewport.
   */
  void fireScroll() {
    if (adapter == null || mPitch == 0 || adapter.getCount() <= MIN_WINDOW) {
      return;
    }
    settle();
  }

  @Override
  public void removeAllViews() {
    super.removeAllViews();
    mFirst = 0;
  }

  /**
   * The rows, as on Android: the live ones, the adapter's and {@link #addItem}'s alike (every row
   * is a widget). A recycling list counts its window, not the adapter.
   */
  @Override
  public int getChildCount() {
    return nativeGetChildCount();
  }

  /** The rows this class added: what {@link #getChildAt} indexes. */
  private int liveRows() {
    return super.getChildCount();
  }

  /**
   * Binds {@code adapter} and builds its rows from scratch: as on Android, a new adapter never sees
   * the old adapter's rows as {@code convertView} (its recycler is cleared), so every position gets
   * {@code getView(position, null, this)} and the previous rows are freed. The list scrolls back to
   * the top.
   */
  @Override
  public void setAdapter(Adapter adapter) {
    removeAllViews();
    mPitch = 0;
    if (mWatching) {
      nativeScrollTo(0, false);
      nativeSetSpacers(0, 0);
    }
    super.setAdapter(adapter);
  }

  @Override
  protected void refreshFromAdapter() {
    if (getChildCount() != liveRows()) {
      // addItem rows, or children added behind the adapter's back: start from an empty list so
      // positions and child indices agree again (a click is resolved by child index).
      removeAllViews();
    }
    int count = adapter == null ? 0 : adapter.getCount();
    if (count == 0) {
      removeAllViews();
      if (mWatching) {
        nativeSetSpacers(0, 0);
      }
      return;
    }
    if (count > MIN_WINDOW && mPitch == 0) {
      // The first row sets the geometry: getHeight runs LVGL's layout pass.
      View first = liveRows() > 0 ? getChildAt(0) : bindSlot(null, mFirst, 0);
      int h = first.getHeight();
      if (h > 0) {
        mPitch = h;
      }
    }
    int window = windowFor(count);
    int live = Math.min(count, window);
    mFirst = clamp(mFirst, 0, count - live);
    for (int k = 0; k < live; k++) {
      bindSlot(k < liveRows() ? getChildAt(k) : null, mFirst + k, k);
    }
    while (liveRows() > live) {
      // Fewer rows frees the surplus, as removeView frees.
      removeView(getChildAt(liveRows() - 1));
    }
    if (count > MIN_WINDOW) {
      if (!mWatching) {
        nativeWatchViewport();
        mWatching = true;
      }
      nativeSetSpacers(mFirst * mPitch, (count - mFirst - live) * mPitch);
      nativeSyncFocusOrder();
    } else if (mWatching) {
      nativeSetSpacers(0, 0);
    }
  }

  /**
   * The row for {@code position} in child slot {@code slot}: {@code convert}, re-bound in place, or
   * the new view the adapter returned instead, which takes the slot while {@code convert} is freed.
   * The only place rows join or leave the child list.
   */
  private View bindSlot(View convert, int position, int slot) {
    View row = adapter.getView(position, convert, this);
    if (row == null) {
      throw new IllegalStateException("getView returned null for position " + position);
    }
    if (row != convert) {
      if (convert != null) {
        removeView(convert);
      }
      addView(row, slot);
      nativeStyleRow(row, mPitch);
    }
    return row;
  }

  /**
   * How many rows cover the viewport with one row beyond each edge, never fewer than {@link
   * #MIN_WINDOW} and never more than the adapter has. Before the list is sized, the minimum: the
   * size change re-evaluates it.
   */
  private int windowFor(int count) {
    int h = getHeight();
    int w = MIN_WINDOW;
    if (h > 0 && mPitch > 0) {
      w = Math.max(MIN_WINDOW, (h + mPitch - 1) / mPitch + 1 + 2 * OVERSCAN);
    }
    return Math.min(count, w);
  }

  /** Moves the window to the rows the scroll offset now shows, re-binding only what changed. */
  private void settle() {
    int count = adapter.getCount();
    int window = windowFor(count);
    int first = clamp(nativeScrollY() / mPitch - OVERSCAN, 0, count - window);
    if (window != liveRows()) {
      // Sized late, or resized: rebuild the window at its new size.
      mFirst = first;
      refreshFromAdapter();
      return;
    }
    if (first == mFirst) {
      return;
    }
    slide(first);
    nativeSetSpacers(first * mPitch, (count - first - window) * mPitch);
    nativeSyncFocusOrder();
  }

  /**
   * Slides the window so child 0 is position {@code first}: the rows leaving one edge are re-bound
   * to the positions entering at the other, one at a time, or all in place for a jump wider than
   * the window.
   */
  private void slide(int first) {
    int n = liveRows();
    int delta = first - mFirst;
    if (Math.abs(delta) >= n) {
      for (int k = 0; k < n; k++) {
        bindSlot(getChildAt(k), first + k, k);
      }
      mFirst = first;
      return;
    }
    boolean toEnd = delta > 0;
    for (int steps = Math.abs(delta); steps > 0; steps--) {
      int from = toEnd ? 0 : n - 1;
      int to = toEnd ? n - 1 : 0;
      int position = toEnd ? mFirst + n : mFirst - 1;
      View row = getChildAt(from);
      nativeRecycleRow(row, toEnd);
      detachViewFromParent(row);
      attachViewToParent(row, to, null);
      bindSlot(row, position, to);
      mFirst += toEnd ? 1 : -1;
    }
  }

  private static int clamp(int v, int lo, int hi) {
    return v < lo ? lo : (v > hi ? hi : v);
  }

  /**
   * Mirrors {@code android.widget.ListView#setSelection}: scrolls so {@code position} is the first
   * row on screen (or as near as the list's end allows). Android also moves the selection there in
   * key navigation; picodroid leaves the keypad focus where it is.
   */
  public void setSelection(int position) {
    scrollToPosition(position, false);
  }

  /**
   * Mirrors {@code android.widget.ListView#smoothScrollToPosition}: {@link #setSelection},
   * animated.
   */
  public void smoothScrollToPosition(int position) {
    scrollToPosition(position, true);
  }

  private void scrollToPosition(int position, boolean smooth) {
    int count = adapter == null ? 0 : adapter.getCount();
    if (count == 0) {
      return;
    }
    position = clamp(position, 0, count - 1);
    if (count <= MIN_WINDOW || mPitch == 0) {
      // Every row is live: the first row's height is the pitch of all, and there is no window
      // to move.
      int h = liveRows() > 0 ? getChildAt(0).getHeight() : 0;
      if (h > 0) {
        nativeScrollTo(position * h, smooth);
      }
      return;
    }
    nativeScrollTo(position * mPitch, smooth);
    if (!smooth) {
      // Synchronous, as Android: the window follows at once rather than on the next frame.
      settle();
    }
  }

  @Override
  public int getFirstVisiblePosition() {
    if (mPitch == 0 || liveRows() == 0) {
      return 0;
    }
    return clamp(nativeScrollY() / mPitch, mFirst, mFirst + liveRows() - 1);
  }

  @Override
  public int getLastVisiblePosition() {
    int count = adapter == null ? 0 : adapter.getCount();
    if (mPitch == 0 || liveRows() == 0) {
      return count - 1;
    }
    int last = (nativeScrollY() + getHeight() - 1) / mPitch;
    return clamp(last, mFirst, mFirst + liveRows() - 1);
  }

  @Override
  public int getPositionForView(View view) {
    int index = super.getPositionForView(view);
    return index < 0 ? INVALID_POSITION : mFirst + index;
  }

  /**
   * Makes a freshly added row behave as a list row: clickable and keypad-focusable, a member of the
   * Activity's focus group, stretched to the list's width, padded, highlighted when focused, and
   * {@code height} pixels tall when that is positive (every row of a recycling list is the height
   * of its first).
   */
  private native void nativeStyleRow(View row, int height);

  /** Asks the widget to report scroll and size changes to {@link #fireScroll}. Idempotent. */
  private native void nativeWatchViewport();

  /** The content's scroll offset in pixels. */
  private native int nativeScrollY();

  /** Scrolls so content row {@code y} is at the top, clamped to the content. */
  private native void nativeScrollTo(int y, boolean smooth);

  /** The padding standing in for the rows above and below the window, in pixels. */
  private native void nativeSetSpacers(int top, int bottom);

  /**
   * Moves {@code row} to the other end of the widget's children ({@code toEnd}: from the top to the
   * bottom), handing the keypad focus to its neighbour first if it held it.
   */
  private native void nativeRecycleRow(View row, boolean toEnd);

  /** Puts the keypad group's rows in child order after rows moved or were replaced. */
  private native void nativeSyncFocusOrder();
}
