// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.ViewGroup;

/**
 * Container that fires a refresh callback on a downward swipe and shows an indeterminate spinner
 * while the caller is doing async work. Wraps a single child view (typically a {@link ScrollView}
 * or {@link ListView}).
 *
 * <p>Lifecycle: a downward swipe → the framework auto-flips refreshing on and calls {@link
 * OnRefreshListener#onRefresh()}. The caller's listener typically kicks off async work and calls
 * {@link #setRefreshing(boolean) setRefreshing(false)} when done.
 */
public class SwipeRefreshLayout extends ViewGroup {
  private OnRefreshListener refreshListener;

  public SwipeRefreshLayout() {
    super(nativeCreate());
  }

  public SwipeRefreshLayout(Context ctx) {
    super(nativeCreate());
  }

  public native void setRefreshing(boolean refreshing);

  public void setOnRefreshListener(OnRefreshListener listener) {
    this.refreshListener = listener;
    nativeRegisterRefreshListener();
  }

  void fireRefresh() {
    if (refreshListener != null) {
      refreshListener.onRefresh();
    }
  }

  private static native int nativeCreate();

  private native void nativeRegisterRefreshListener();

  /** Mirrors {@code androidx.swiperefreshlayout.widget.SwipeRefreshLayout.OnRefreshListener}. */
  public interface OnRefreshListener {
    void onRefresh();
  }
}
