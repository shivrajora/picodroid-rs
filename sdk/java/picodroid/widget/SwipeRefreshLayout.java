package picodroid.widget;

import picodroid.view.View;

/**
 * Container that fires a refresh callback on a downward swipe and shows an indeterminate spinner
 * while the caller is doing async work. Wraps a single child view (typically a {@link ScrollView}
 * or {@link ListView}).
 *
 * <p>Lifecycle: a downward swipe → the framework auto-flips refreshing on and calls {@link
 * #fireRefresh()}. The caller's {@link Runnable} listener typically kicks off async work and calls
 * {@link #setRefreshing(boolean) setRefreshing(false)} when done.
 */
public class SwipeRefreshLayout extends View {
  private Runnable refreshListener;

  public SwipeRefreshLayout() {
    super(nativeCreate());
  }

  public native void addView(View child);

  public native void setRefreshing(boolean refreshing);

  public void setOnRefreshListener(Runnable listener) {
    this.refreshListener = listener;
    nativeRegisterRefreshListener();
  }

  void fireRefresh() {
    if (refreshListener != null) {
      refreshListener.run();
    }
  }

  private static native int nativeCreate();

  private native void nativeRegisterRefreshListener();
}
