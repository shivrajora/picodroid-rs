// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.app.Activity;
import picodroid.app.Fragment;
import picodroid.app.FragmentManager;
import picodroid.os.Bundle;
import picodroid.view.View;

/**
 * Supplies a {@link ViewPager2} with one {@link Fragment} per page, mirroring {@code
 * androidx.viewpager2.adapter.FragmentStateAdapter}: implement {@link #getItemCount} and {@link
 * #createFragment}. The pager keeps one page alive; when it turns, the outgoing fragment's {@code
 * onSaveInstanceState} Bundle is kept here under its {@link #getItemId item id} and the fragment is
 * removed, and when that page returns {@link #createFragment} makes a new instance that gets the
 * Bundle back through {@link Fragment#setInitialSavedState}. Pages are added to the {@link
 * FragmentManager} with the tag {@code "f" + itemId}, Android's spelling. As on Android that tag is
 * an implementation detail, not an API: a host that looks its pages up by it is reaching past the
 * adapter. Let each page observe what it shows (a {@link picodroid.lifecycle.LiveData} from a
 * shared {@link picodroid.lifecycle.ViewModel}) instead of pushing data into the current page.
 *
 * <p>Page fragments are not saved with the Activity's other fragments (the manager cannot place
 * them again); the Activity saves them through {@link ViewPager2#saveState}. Android's {@code
 * (Fragment)} constructor is not offered: there are no child fragment managers.
 */
public abstract class FragmentStateAdapter {
  private final FragmentManager mFragmentManager;
  private ViewPager2 mPager;

  /** The one live page, or null. */
  private Fragment mFragment;

  private long mFragmentId;

  /** Saved states of pages that are not live, by item id. */
  private long[] mStateIds = new long[4];

  private Bundle[] mStates = new Bundle[4];
  private int mStateCount;

  /** Pages hosted by {@code activity}'s fragment manager. Android takes a FragmentActivity. */
  public FragmentStateAdapter(Activity activity) {
    this(activity.getSupportFragmentManager());
  }

  /** Pages hosted by {@code fragmentManager}. Android also takes the host's Lifecycle. */
  public FragmentStateAdapter(FragmentManager fragmentManager) {
    mFragmentManager = fragmentManager;
  }

  /** A new fragment for page {@code position}; the pager adds it and places its view. */
  public abstract Fragment createFragment(int position);

  public abstract int getItemCount();

  /** A stable id for the page at {@code position}; the position itself unless pages move. */
  public long getItemId(int position) {
    return position;
  }

  /** Whether {@code itemId} still names a page; a saved state for one that does not is dropped. */
  public boolean containsItem(long itemId) {
    return itemId >= 0 && itemId < getItemCount();
  }

  /** The page set changed: the pager re-clamps its current page on the next tick. */
  public final void notifyDataSetChanged() {
    if (mPager != null) {
      mPager.dataSetChanged();
    }
  }

  /** Every page's saved state, the live page's included, or null when there is none. */
  public final Bundle saveState() {
    Bundle out = new Bundle();
    int n = 0;
    if (mFragment != null && mFragment.isAdded() && containsItem(mFragmentId)) {
      Bundle live = mFragmentManager.saveFragmentInstanceState(mFragment);
      if (live != null) {
        out.putLong("id0", mFragmentId);
        out.putBundle("st0", live);
        n = 1;
      }
    }
    for (int i = 0; i < mStateCount; i++) {
      if (containsItem(mStateIds[i])) {
        out.putLong("id" + n, mStateIds[i]);
        out.putBundle("st" + n, mStates[i]);
        n++;
      }
    }
    if (n == 0) {
      return null;
    }
    out.putInt("n", n);
    return out;
  }

  /** Mirrors Android: only on a fresh adapter, before it has made a page. */
  public final void restoreState(Bundle savedState) {
    if (mFragment != null || mStateCount > 0) {
      throw new IllegalStateException("Expected the adapter to be 'fresh' while restoring state.");
    }
    int n = savedState.getInt("n", 0);
    for (int i = 0; i < n; i++) {
      putState(savedState.getLong("id" + i, -1L), savedState.getBundle("st" + i));
    }
  }

  // ── Pager hooks ─────────────────────────────────────────────────────────

  void attach(ViewPager2 pager) {
    mPager = pager;
  }

  boolean isStateSaved() {
    return mFragmentManager.isStateSaved();
  }

  /** Create the page at {@code position} at STARTED and return its view for the pager to place. */
  View onBindPage(int position) {
    Fragment f = createFragment(position);
    long id = getItemId(position);
    Bundle saved = takeState(id);
    if (saved != null) {
      f.setInitialSavedState(saved);
    }
    mFragmentManager
        .beginTransaction()
        .add(f, "f" + id)
        .setMaxLifecycle(f, Fragment.STARTED)
        .commitNow();
    View view = f.getView();
    if (view == null) {
      mFragmentManager.beginTransaction().remove(f).commitNow();
      throw new IllegalStateException(
          "ViewPager2 page "
              + position
              + " has no view: onCreateView returned null, or the adapter was set before the"
              + " Activity started");
    }
    mFragment = f;
    mFragmentId = id;
    return view;
  }

  /** The bound page is now the one on screen: resume it. */
  void onPrimaryPage() {
    Fragment f = mFragment;
    if (f != null && f.isAdded() && !f.isResumed()) {
      mFragmentManager.beginTransaction().setMaxLifecycle(f, Fragment.RESUMED).commitNow();
    }
  }

  /** Keep the bound page's state and remove it; its view goes with it. */
  void onUnbindPage() {
    Fragment f = mFragment;
    if (f == null) {
      return;
    }
    if (f.isAdded()) {
      if (containsItem(mFragmentId)) {
        putState(mFragmentId, mFragmentManager.saveFragmentInstanceState(f));
      }
      mFragmentManager.beginTransaction().remove(f).commitNow();
    }
    mFragment = null;
  }

  private void putState(long id, Bundle state) {
    for (int i = 0; i < mStateCount; i++) {
      if (mStateIds[i] == id) {
        if (state == null) {
          removeStateAt(i);
        } else {
          mStates[i] = state;
        }
        return;
      }
    }
    if (state == null) {
      return;
    }
    if (mStateCount == mStateIds.length) {
      long[] ids = new long[mStateIds.length * 2];
      System.arraycopy(mStateIds, 0, ids, 0, mStateCount);
      mStateIds = ids;
      Bundle[] states = new Bundle[mStates.length * 2];
      System.arraycopy(mStates, 0, states, 0, mStateCount);
      mStates = states;
    }
    mStateIds[mStateCount] = id;
    mStates[mStateCount] = state;
    mStateCount++;
  }

  private Bundle takeState(long id) {
    for (int i = 0; i < mStateCount; i++) {
      if (mStateIds[i] == id) {
        Bundle state = mStates[i];
        removeStateAt(i);
        return state;
      }
    }
    return null;
  }

  private void removeStateAt(int i) {
    int tail = mStateCount - i - 1;
    System.arraycopy(mStateIds, i + 1, mStateIds, i, tail);
    System.arraycopy(mStates, i + 1, mStates, i, tail);
    mStateCount--;
    mStates[mStateCount] = null;
  }
}
