// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.concurrent.Executors;
import picodroid.content.Context;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.OnSwipeListener;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * Pages of fragments, one on screen at a time, mirroring {@code
 * androidx.viewpager2.widget.ViewPager2} with a {@link FragmentStateAdapter}:
 *
 * <pre>{@code
 * ViewPager2 pager = findViewById(R.id.pager);
 * pager.setAdapter(new FragmentStateAdapter(this) {
 *   @Override public int getItemCount() { return 4; }
 *   @Override public Fragment createFragment(int position) { return PageFragment.newInstance(position); }
 * });
 * pager.registerOnPageChangeCallback(new ViewPager2.OnPageChangeCallback() {
 *   @Override public void onPageSelected(int position) { dots.select(position); }
 * });
 * }</pre>
 *
 * <p>An embedded panel has no room for the page beside the current one, so this pager keeps exactly
 * one page alive: a turn removes the outgoing fragment (its state kept as a Bundle, its widgets
 * freed) and creates the incoming one, over three main-thread ticks so no single tick carries the
 * whole turn. There is no scroller either: {@code smoothScroll} is a fade of the incoming page, the
 * scroll state goes {@link #SCROLL_STATE_SETTLING} then {@link #SCROLL_STATE_IDLE}, {@link
 * #SCROLL_STATE_DRAGGING} is never reported, and {@link OnPageChangeCallback#onPageScrolled} is
 * called once per turn with a zero offset. {@link #setOffscreenPageLimit} is accepted and logged;
 * {@link #OFFSCREEN_PAGE_LIMIT_DEFAULT} is the only behaviour. A swipe across the page turns it
 * when {@link #setUserInputEnabled user input} is on; on a board with keys, call {@link
 * #setCurrentItem} from {@code onKeyDown}. The page index and the pages' saved states are yours to
 * keep: {@link #saveState} into the Activity's Bundle and {@link #restoreState} in {@code
 * onCreate}, before {@link #setAdapter}.
 *
 * <p>Not here: page transformers, fake drags, item decorations, {@code AttributeSet} constructors,
 * {@code android:orientation} in XML.
 */
public class ViewPager2 extends FrameLayout {
  public static final int ORIENTATION_HORIZONTAL = 0;
  public static final int ORIENTATION_VERTICAL = 1;
  public static final int SCROLL_STATE_IDLE = 0;
  public static final int SCROLL_STATE_DRAGGING = 1;
  public static final int SCROLL_STATE_SETTLING = 2;
  public static final int OFFSCREEN_PAGE_LIMIT_DEFAULT = -1;

  /** The fade an incoming page gets for {@code smoothScroll}, in ms. */
  static final int FADE_MS = 180;

  private static final String TAG = "ViewPager2";

  /** Mirrors Android: override what you need; every method here does nothing. */
  public abstract static class OnPageChangeCallback {
    public OnPageChangeCallback() {}

    public void onPageScrolled(int position, float positionOffset, int positionOffsetPixels) {}

    public void onPageSelected(int position) {}

    public void onPageScrollStateChanged(int state) {}
  }

  private FragmentStateAdapter mAdapter;

  /** The page asked for; {@link #mBoundItem} catches up over the swap ticks. */
  private int mCurrentItem;

  private int mBoundItem = -1;
  private View mBoundView;
  private boolean mSwapping;
  private boolean mSmooth;
  private int mScrollState = SCROLL_STATE_IDLE;
  private int mOffscreenPageLimit = OFFSCREEN_PAGE_LIMIT_DEFAULT;
  private boolean mUserInputEnabled = true;
  private int mOrientation = ORIENTATION_HORIZONTAL;
  private OnPageChangeCallback[] mCallbacks;
  private int mCallbackCount;
  private Bundle mPendingAdapterState;

  /** On the pager and on every page root: the pressed object gets LVGL's gesture. */
  private final OnSwipeListener mSwipeListener = (view, direction) -> onSwipe(direction);

  public ViewPager2(Context ctx) {
    super(ctx);
    setOnSwipeListener(mSwipeListener);
  }

  /** Mirrors Android: the adapter that creates the pages; the first page appears a tick later. */
  public void setAdapter(FragmentStateAdapter adapter) {
    if (mAdapter != null && mBoundItem >= 0) {
      unbind();
    }
    mAdapter = adapter;
    mSwapping = false;
    if (adapter == null) {
      return;
    }
    adapter.attach(this);
    if (mPendingAdapterState != null) {
      adapter.restoreState(mPendingAdapterState);
      mPendingAdapterState = null;
    }
    int count = adapter.getItemCount();
    if (mCurrentItem >= count) {
      mCurrentItem = count > 0 ? count - 1 : 0;
    }
    if (count > 0) {
      startSwap(false);
    }
  }

  public FragmentStateAdapter getAdapter() {
    return mAdapter;
  }

  /** Mirrors Android: {@code setCurrentItem(item, true)}. */
  public void setCurrentItem(int item) {
    setCurrentItem(item, true);
  }

  /**
   * Turn to page {@code item}. With {@code smoothScroll} the new page fades in, and the scroll
   * state reports SETTLING until it has. A turn asked for while one is under way retargets it.
   */
  public void setCurrentItem(int item, boolean smoothScroll) {
    if (mAdapter == null || mAdapter.getItemCount() <= 0) {
      mCurrentItem = item; // applied when an adapter with pages arrives, as on Android
      return;
    }
    int count = mAdapter.getItemCount();
    if (item < 0) {
      item = 0;
    } else if (item >= count) {
      item = count - 1;
    }
    if (item == mCurrentItem && mBoundItem == item && !mSwapping) {
      return;
    }
    mCurrentItem = item;
    if (mSwapping) {
      if (smoothScroll && !mSmooth) {
        mSmooth = true;
        setScrollState(SCROLL_STATE_SETTLING);
      }
      return;
    }
    startSwap(smoothScroll);
  }

  /** The page turned to, even while the turn is still under way, as on Android. */
  public int getCurrentItem() {
    return mCurrentItem;
  }

  /**
   * Accepted for source compatibility: this pager keeps one page alive whatever the limit, and logs
   * a value other than {@link #OFFSCREEN_PAGE_LIMIT_DEFAULT} once.
   */
  public void setOffscreenPageLimit(int limit) {
    if (limit < 1 && limit != OFFSCREEN_PAGE_LIMIT_DEFAULT) {
      throw new IllegalArgumentException(
          "Offscreen page limit must be OFFSCREEN_PAGE_LIMIT_DEFAULT or a number > 0");
    }
    mOffscreenPageLimit = limit;
    if (limit != OFFSCREEN_PAGE_LIMIT_DEFAULT) {
      Log.w(TAG, "offscreenPageLimit " + limit + " not honoured: one page at a time");
    }
  }

  public int getOffscreenPageLimit() {
    return mOffscreenPageLimit;
  }

  /** Whether a swipe turns the page. Off on a board without touch, where keys drive the pager. */
  public void setUserInputEnabled(boolean enabled) {
    mUserInputEnabled = enabled;
  }

  public boolean isUserInputEnabled() {
    return mUserInputEnabled;
  }

  /** Which swipes turn the page: left/right for horizontal, up/down for vertical. */
  public void setOrientation(int orientation) {
    if (orientation != ORIENTATION_HORIZONTAL && orientation != ORIENTATION_VERTICAL) {
      throw new IllegalArgumentException("orientation must be ORIENTATION_HORIZONTAL or VERTICAL");
    }
    mOrientation = orientation;
  }

  public int getOrientation() {
    return mOrientation;
  }

  public int getScrollState() {
    return mScrollState;
  }

  public void registerOnPageChangeCallback(OnPageChangeCallback callback) {
    if (mCallbacks == null) {
      mCallbacks = new OnPageChangeCallback[2];
    } else if (mCallbackCount == mCallbacks.length) {
      OnPageChangeCallback[] bigger = new OnPageChangeCallback[mCallbacks.length * 2];
      System.arraycopy(mCallbacks, 0, bigger, 0, mCallbackCount);
      mCallbacks = bigger;
    }
    mCallbacks[mCallbackCount++] = callback;
  }

  public void unregisterOnPageChangeCallback(OnPageChangeCallback callback) {
    for (int i = 0; i < mCallbackCount; i++) {
      if (mCallbacks[i] == callback) {
        System.arraycopy(mCallbacks, i + 1, mCallbacks, i, mCallbackCount - i - 1);
        mCallbacks[--mCallbackCount] = null;
        return;
      }
    }
  }

  /**
   * The current page index and the adapter's saved page states, for the Activity's {@code
   * onSaveInstanceState}. Android saves these through the view hierarchy; there is none here.
   */
  public Bundle saveState() {
    Bundle out = new Bundle();
    out.putInt("currentItem", mCurrentItem);
    if (mAdapter != null) {
      Bundle adapter = mAdapter.saveState();
      if (adapter != null) {
        out.putBundle("adapter", adapter);
      }
    }
    return out;
  }

  /** The counterpart of {@link #saveState}; call it before or after {@link #setAdapter}. */
  public void restoreState(Bundle state) {
    if (state == null) {
      return;
    }
    mCurrentItem = state.getInt("currentItem", 0);
    Bundle adapter = state.getBundle("adapter");
    if (mAdapter == null) {
      mPendingAdapterState = adapter;
      return;
    }
    if (adapter != null) {
      mAdapter.restoreState(adapter);
    }
    dataSetChanged();
  }

  // ── The swap, one step per main-thread tick ─────────────────────────────

  private static void post(Runnable r) {
    Executors.mainExecutor().execute(r);
  }

  private void startSwap(boolean smooth) {
    mSwapping = true;
    mSmooth = smooth;
    if (smooth) {
      setScrollState(SCROLL_STATE_SETTLING);
    }
    post(() -> swapUnbind());
  }

  /** Tick 1: free the outgoing page, so its widgets are gone before the new page's exist. */
  private void swapUnbind() {
    if (!mSwapping || mAdapter == null || mAdapter.isStateSaved()) {
      mSwapping = false;
      return;
    }
    if (mBoundItem >= 0 && mBoundItem != mCurrentItem) {
      unbind();
    }
    post(() -> swapBind());
  }

  /** Tick 2: create the incoming page at STARTED and put its view in place. */
  private void swapBind() {
    if (!mSwapping || mAdapter == null || mAdapter.isStateSaved()) {
      mSwapping = false;
      return;
    }
    if (mBoundItem < 0) {
      int count = mAdapter.getItemCount();
      if (mCurrentItem >= count) {
        mCurrentItem = count - 1;
      }
      if (mCurrentItem < 0) {
        mSwapping = false;
        return;
      }
      View page = mAdapter.onBindPage(mCurrentItem);
      if (mSmooth) {
        page.setAlpha(0f);
      }
      // A page carries its size in its LayoutParams (an inflated one always, one built in code
      // through setLayoutParams); the two-argument add applies them.
      ViewGroup.LayoutParams lp = page.getLayoutParams();
      if (lp != null) {
        addView(page, lp);
      } else {
        addView(page);
      }
      page.setOnSwipeListener(mSwipeListener);
      mBoundItem = mCurrentItem;
      mBoundView = page;
    }
    post(() -> swapPromote());
  }

  /** Tick 3: resume the page, tell the callbacks, fade it in if asked. */
  private void swapPromote() {
    if (!mSwapping || mAdapter == null || mAdapter.isStateSaved()) {
      mSwapping = false;
      return;
    }
    if (mBoundItem != mCurrentItem) {
      post(() -> swapUnbind()); // retargeted while binding: go round again
      return;
    }
    mAdapter.onPrimaryPage();
    mSwapping = false;
    dispatchSelected(mCurrentItem);
    if (mSmooth && mBoundView != null) {
      mBoundView.animate().alpha(1f).setDuration(FADE_MS).withEndAction(() -> settled()).start();
    } else {
      settled();
    }
  }

  private void settled() {
    if (mSwapping) {
      return; // a newer turn owns the screen
    }
    dispatchScrolled(mCurrentItem);
    setScrollState(SCROLL_STATE_IDLE);
  }

  private void unbind() {
    View page = mBoundView;
    mAdapter.onUnbindPage();
    if (page != null && page.getParent() == this) {
      removeView(page); // the fragment's own teardown normally freed it already
    }
    mBoundItem = -1;
    mBoundView = null;
  }

  /** The adapter's item set changed: re-clamp, drop a page that is gone, bind the current one. */
  void dataSetChanged() {
    post(() -> layout());
  }

  private void layout() {
    if (mAdapter == null) {
      return;
    }
    int count = mAdapter.getItemCount();
    if (mCurrentItem >= count) {
      mCurrentItem = count > 0 ? count - 1 : 0;
    }
    if (mBoundItem >= 0
        && (mBoundItem >= count || !mAdapter.containsItem(mAdapter.getItemId(mBoundItem)))) {
      unbind();
    }
    if (!mSwapping && mBoundItem != mCurrentItem && count > 0) {
      startSwap(false);
    }
  }

  private void onSwipe(int direction) {
    if (!mUserInputEnabled || mAdapter == null) {
      return;
    }
    int by;
    if (mOrientation == ORIENTATION_HORIZONTAL) {
      if (direction == View.SWIPE_LEFT) {
        by = 1;
      } else if (direction == View.SWIPE_RIGHT) {
        by = -1;
      } else {
        return;
      }
    } else {
      if (direction == View.SWIPE_UP) {
        by = 1;
      } else if (direction == View.SWIPE_DOWN) {
        by = -1;
      } else {
        return;
      }
    }
    int target = mCurrentItem + by;
    if (target < 0 || target >= mAdapter.getItemCount()) {
      return; // no wrap, as on Android
    }
    setCurrentItem(target, true);
  }

  private void setScrollState(int state) {
    if (mScrollState == state) {
      return;
    }
    mScrollState = state;
    for (int i = 0; i < mCallbackCount; i++) {
      mCallbacks[i].onPageScrollStateChanged(state);
    }
  }

  private void dispatchSelected(int position) {
    for (int i = 0; i < mCallbackCount; i++) {
      mCallbacks[i].onPageSelected(position);
    }
  }

  private void dispatchScrolled(int position) {
    for (int i = 0; i < mCallbackCount; i++) {
      mCallbacks[i].onPageScrolled(position, 0f, 0);
    }
  }
}
