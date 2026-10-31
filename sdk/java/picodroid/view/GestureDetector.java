// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

import java.util.concurrent.RejectedExecutionException;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Context;

/**
 * Mirrors {@code android.view.GestureDetector}: turns a view's touch stream into down, show-press,
 * single-tap, scroll, long-press and fling callbacks. Feed it every event, as on Android:
 *
 * <pre>{@code
 * GestureDetector detector = new GestureDetector(this, listener);
 * view.setOnTouchListener((v, e) -> detector.onTouchEvent(e));
 * }</pre>
 *
 * <p>or call {@link #onTouchEvent} from a custom view's {@link View#onTouchEvent}. The press timers
 * run on {@link Executors#mainScheduledExecutor()} (there is no {@code Handler} here): {@link
 * OnGestureListener#onShowPress} 100 ms after the press and {@link OnGestureListener#onLongPress}
 * at {@link ViewConfiguration#getLongPressTimeout()}, both only while the finger stays in the tap
 * region. When the timer table is full, the touch panel's own long-press event ({@link
 * MotionEvent#ACTION_LONG_PRESS}, at the same timeout) stands in.
 *
 * <p>Picodroid divergences: the tap region is 12 px and a fling needs 24 px of travel, both on the
 * dominant axis and in raw (screen) coordinates, so a view that moves under the finger does not
 * skew them; the fling velocity is the slope of the last sample before the release, in pixels per
 * second. Single touch; no double tap ({@code OnDoubleTapListener}) and no {@code Handler}
 * constructor.
 */
public class GestureDetector {
  /** Max travel from the press, in pixels, that still counts as a tap. */
  private static final int TAP_SLOP_PX = 12;

  /** Min travel from the press, in pixels, for a release to count as a fling. */
  private static final int FLING_MIN_PX = 24;

  /** Positions the fling slope spans: about 50 ms of panel samples. */
  private static final int SAMPLES = 4;

  /** Android's {@code ViewConfiguration.getTapTimeout()}: the time to {@code onShowPress}. */
  private static final int TAP_TIMEOUT_MS = 100;

  /** Mirrors {@code android.view.GestureDetector.OnGestureListener}. */
  public interface OnGestureListener {
    /**
     * Every press starts here. Return true to consume the gesture, which also keeps the view's
     * click from firing.
     */
    boolean onDown(MotionEvent e);

    /** The finger is still down, and has not moved, 100 ms after the press. */
    void onShowPress(MotionEvent e);

    /** A release inside the tap region, before the long-press timeout. */
    boolean onSingleTapUp(MotionEvent e);

    /**
     * A move once the finger has left the tap region. {@code distanceX} / {@code distanceY} are the
     * travel since the last call, previous minus current, as on Android.
     */
    boolean onScroll(MotionEvent e1, MotionEvent e2, float distanceX, float distanceY);

    /** The press, held in the tap region for the long-press timeout; no tap follows it. */
    void onLongPress(MotionEvent e);

    /** A release after a swipe; velocities in pixels per second, positive right and down. */
    boolean onFling(MotionEvent e1, MotionEvent e2, float velocityX, float velocityY);
  }

  /**
   * Mirrors {@code android.view.GestureDetector.SimpleOnGestureListener}: every callback does
   * nothing and returns false, so a subclass overrides only the gestures it wants.
   */
  public static class SimpleOnGestureListener implements OnGestureListener {
    @Override
    public boolean onDown(MotionEvent e) {
      return false;
    }

    @Override
    public void onShowPress(MotionEvent e) {}

    @Override
    public boolean onSingleTapUp(MotionEvent e) {
      return false;
    }

    @Override
    public boolean onScroll(MotionEvent e1, MotionEvent e2, float distanceX, float distanceY) {
      return false;
    }

    @Override
    public void onLongPress(MotionEvent e) {}

    @Override
    public boolean onFling(MotionEvent e1, MotionEvent e2, float velocityX, float velocityY) {
      return false;
    }
  }

  private final OnGestureListener mListener;
  private boolean mIsLongpressEnabled = true;

  // The gesture in progress: single touch.
  private MotionEvent mCurrentDownEvent; // a copy: the runtime reuses the event it dispatches
  // The finger's last distinct positions (raw coordinates, event time), a ring: the fling slope.
  private final int[] mSampleX = new int[SAMPLES];
  private final int[] mSampleY = new int[SAMPLES];
  private final long[] mSampleTime = new long[SAMPLES];
  private int mSampleHead; // the latest
  private int mSampleCount;
  private int mLastX; // raw coordinates of the last onScroll, for its distances
  private int mLastY;
  private boolean mStillDown;
  private boolean mAlwaysInTapRegion;
  private boolean mShowPressSent;
  private boolean mInLongPress;

  /** The show-press timer, then the long-press one; null when neither is armed. */
  private ScheduledFuture<?> mTimer;

  private final Runnable mTimerTask = this::onTimer;

  /**
   * Mirrors Android's {@code GestureDetector(Context, OnGestureListener)}.
   *
   * @throws NullPointerException if {@code listener} is null
   */
  public GestureDetector(Context context, OnGestureListener listener) {
    if (listener == null) {
      throw new NullPointerException("OnGestureListener must not be null");
    }
    mListener = listener;
  }

  /** Whether a held press becomes {@code onLongPress}; on by default. */
  public void setIsLongpressEnabled(boolean isLongpressEnabled) {
    mIsLongpressEnabled = isLongpressEnabled;
  }

  public boolean isLongpressEnabled() {
    return mIsLongpressEnabled;
  }

  /**
   * Analyzes one event of the stream and calls the listener. Returns what the callback it reached
   * returned (false when none was reached), for a touch listener or {@code onTouchEvent} to return.
   */
  public boolean onTouchEvent(MotionEvent ev) {
    int action = ev.getAction();
    if (action == MotionEvent.ACTION_DOWN) {
      cancelTimer();
      mCurrentDownEvent = MotionEvent.obtain(ev);
      mSampleCount = 0;
      sample(ev);
      mLastX = ev.getRawX();
      mLastY = ev.getRawY();
      mStillDown = true;
      mAlwaysInTapRegion = true;
      mShowPressSent = false;
      mInLongPress = false;
      arm(TAP_TIMEOUT_MS);
      return mListener.onDown(ev);
    }
    MotionEvent down = mCurrentDownEvent;
    if (down == null) {
      return false; // no press seen: the stream started mid-gesture
    }
    if (action == MotionEvent.ACTION_MOVE) {
      if (mInLongPress) {
        return false;
      }
      sample(ev);
      int x = ev.getRawX();
      int y = ev.getRawY();
      if (mAlwaysInTapRegion && travel(down, ev) <= TAP_SLOP_PX) {
        return false;
      }
      if (x == mLastX && y == mLastY) {
        return false;
      }
      if (mAlwaysInTapRegion) {
        mAlwaysInTapRegion = false;
        cancelTimer();
      }
      float distanceX = mLastX - x;
      float distanceY = mLastY - y;
      mLastX = x;
      mLastY = y;
      return mListener.onScroll(down, ev, distanceX, distanceY);
    }
    if (action == MotionEvent.ACTION_UP) {
      boolean longPressed = mInLongPress;
      boolean tap = mAlwaysInTapRegion;
      cancelTimer();
      mStillDown = false;
      mInLongPress = false;
      mCurrentDownEvent = null;
      if (longPressed) {
        return false;
      }
      if (tap) {
        return mListener.onSingleTapUp(ev);
      }
      if (travel(down, ev) < FLING_MIN_PX) {
        return false;
      }
      // The slope over the finger's last few positions before the release (some 50 ms of
      // samples), so a flick whose speed comes from its final frames reads as fast without one
      // jittery step setting the direction; the travel test above used the whole press, so a
      // slow drift with a quick lift is not a fling.
      sample(ev);
      int last = mSampleHead;
      int first = (last + SAMPLES - (mSampleCount - 1)) % SAMPLES;
      long durationMs = mSampleTime[last] - mSampleTime[first];
      if (durationMs <= 0) {
        durationMs = 1;
      }
      float vx = (float) (mSampleX[last] - mSampleX[first]) * 1000.0f / (float) durationMs;
      float vy = (float) (mSampleY[last] - mSampleY[first]) * 1000.0f / (float) durationMs;
      return mListener.onFling(down, ev, vx, vy);
    }
    if (action == MotionEvent.ACTION_LONG_PRESS) {
      // The panel's own long press, at the same timeout: it stands in for a timer that could not
      // be armed. Consumed once a long press has fired, so the release does not click.
      if (mIsLongpressEnabled && mStillDown && mAlwaysInTapRegion && !mInLongPress) {
        dispatchLongPress();
      }
      return mInLongPress;
    }
    return false;
  }

  /**
   * Records the finger's position in the ring; one on the spot of the latest only moves its time,
   * so a pause before the release reads as slow.
   */
  private void sample(MotionEvent ev) {
    int x = ev.getRawX();
    int y = ev.getRawY();
    int head = mSampleHead;
    if (mSampleCount == 0 || x != mSampleX[head] || y != mSampleY[head]) {
      head = (head + 1) % SAMPLES;
      mSampleHead = head;
      mSampleX[head] = x;
      mSampleY[head] = y;
      if (mSampleCount < SAMPLES) {
        mSampleCount++;
      }
    }
    mSampleTime[head] = ev.getEventTime();
  }

  /** Travel from the press on the dominant axis, in raw pixels. */
  private static int travel(MotionEvent from, MotionEvent to) {
    int dx = to.getRawX() - from.getRawX();
    int dy = to.getRawY() - from.getRawY();
    int absDx = dx < 0 ? -dx : dx;
    int absDy = dy < 0 ? -dy : dy;
    return absDx > absDy ? absDx : absDy;
  }

  private void arm(long delayMs) {
    try {
      mTimer =
          Executors.mainScheduledExecutor().schedule(mTimerTask, delayMs, TimeUnit.MILLISECONDS);
    } catch (RejectedExecutionException e) {
      mTimer = null; // the table is full: the panel's long-press event stands in
    }
  }

  private void cancelTimer() {
    ScheduledFuture<?> timer = mTimer;
    if (timer != null) {
      mTimer = null;
      timer.cancel(false);
    }
  }

  private void onTimer() {
    mTimer = null;
    if (!mStillDown || !mAlwaysInTapRegion || mInLongPress) {
      return;
    }
    if (!mShowPressSent) {
      mShowPressSent = true;
      mListener.onShowPress(mCurrentDownEvent);
      if (mIsLongpressEnabled) {
        arm(ViewConfiguration.getLongPressTimeout() - TAP_TIMEOUT_MS);
      }
    } else if (mIsLongpressEnabled) {
      dispatchLongPress();
    }
  }

  private void dispatchLongPress() {
    cancelTimer();
    MotionEvent down = mCurrentDownEvent;
    if (!mShowPressSent) {
      mShowPressSent = true;
      mListener.onShowPress(down);
    }
    mInLongPress = true;
    mListener.onLongPress(down);
  }
}
