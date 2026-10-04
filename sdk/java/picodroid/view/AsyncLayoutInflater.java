// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

import picodroid.concurrent.Executor;
import picodroid.concurrent.Executors;
import picodroid.content.Context;
import picodroid.os.SystemClock;

/**
 * Inflates a layout without holding up the main thread, and hands the finished tree to a callback
 * on it. Mirrors {@code androidx.asynclayoutinflater.view.AsyncLayoutInflater}:
 *
 * <pre>{@code
 * new AsyncLayoutInflater(context)
 *     .inflate(R.layout.page_limits, container, (view, resid, parent) -> {
 *       parent.addView(view);
 *       ...
 *     });
 * }</pre>
 *
 * <p>Android inflates on a worker thread. Views are made on the main thread here, so the work is
 * cut into slices instead: each tick of the main loop builds views for a few milliseconds and
 * leaves the rest to the next tick, which keeps input and animation alive while a screen of thirty
 * views is built on a board where each one costs milliseconds. The callback runs on a tick of its
 * own once the tree is whole. As on Android the view is not added to {@code parent}, which is only
 * there to make the root's {@code LayoutParams}; until the callback the tree is kept hidden.
 *
 * <p>An {@code OutOfMemoryError} part-way drops what was built and starts again on a later tick,
 * after the collector has had its chance; the error is thrown only if that keeps happening.
 *
 * <p>A callback that finds its screen gone (a fragment whose view was destroyed meanwhile) should
 * {@link View#close() close} the view: picodroid frees a view's widget when it is removed, and this
 * one was never added.
 */
public final class AsyncLayoutInflater {
  /** How long one tick spends building views before it leaves the rest to the next. */
  private static final int SLICE_MS = 8;

  /** How many times an inflation that ran out of memory is started again. */
  private static final int RETRIES = 3;

  private final Context mContext;

  /** Mirrors {@code AsyncLayoutInflater.OnInflateFinishedListener}. */
  public interface OnInflateFinishedListener {
    /** {@code view} is the inflated {@code resid}; it has not been added to {@code parent}. */
    void onInflateFinished(View view, int resid, ViewGroup parent);
  }

  public AsyncLayoutInflater(Context context) {
    mContext = context;
  }

  /**
   * Inflates {@code resid} over the coming ticks and then calls {@code callback} on the main
   * thread. Call it on the main thread.
   */
  public void inflate(int resid, ViewGroup parent, OnInflateFinishedListener callback) {
    if (callback == null) {
      throw new NullPointerException("callback argument may not be null!");
    }
    Executors.mainExecutor().execute(new Request(mContext, resid, parent, callback));
  }

  /** One inflation: the main-queue task that runs its slices and then its callback. */
  private static final class Request implements Runnable {
    private final LayoutInflater inflater;
    private final Executor main = Executors.mainExecutor();
    private final int resid;
    private final ViewGroup parent;
    private final OnInflateFinishedListener callback;
    private boolean begun;
    private boolean built;
    private int failures;

    Request(Context context, int resid, ViewGroup parent, OnInflateFinishedListener callback) {
      // An inflater of its own: one holds the read position of the layout it is inflating.
      this.inflater = LayoutInflater.from(context);
      this.resid = resid;
      this.parent = parent;
      this.callback = callback;
    }

    @Override
    public void run() {
      if (built) {
        callback.onInflateFinished(inflater.finish(), resid, parent);
        return;
      }
      try {
        if (!begun) {
          inflater.begin(resid, parent);
          begun = true;
        }
        built = !inflater.resume(SystemClock.elapsedRealtime() + SLICE_MS);
      } catch (OutOfMemoryError e) {
        inflater.abandon();
        begun = false;
        if (++failures > RETRIES) {
          throw e;
        }
      }
      main.execute(this); // the next slice, the retry, or the callback: each on its own tick
    }
  }
}
