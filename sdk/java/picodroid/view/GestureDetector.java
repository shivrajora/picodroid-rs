// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Minimal gesture recognizer modeled on Android's {@code GestureDetector}.
 *
 * <p>Hook this up via {@link View#setOnTouchListener}; it keeps just enough state to dispatch four
 * gestures off the touch event stream:
 *
 * <ul>
 *   <li>{@link OnGestureListener#onSingleTap} — DOWN then UP within {@link #TAP_SLOP_PX} pixels and
 *       under the LVGL long-press threshold (no LONG_PRESS event arrived first).
 *   <li>{@link OnGestureListener#onLongPress} — fired *while the finger is still down* when LVGL
 *       emits its long-press event. Subsequent UP suppresses {@code onSingleTap}.
 *   <li>{@link OnGestureListener#onFling} — UP whose displacement from DOWN exceeds {@link
 *       #FLING_MIN_PX}; velocity is computed from the lift-off slope (last MOVE → UP) so a flick
 *       whose speed comes from the final frames reads as fast even if the gesture was initially
 *       slow. Tap / fling classification still uses the full DOWN→UP displacement.
 * </ul>
 *
 * <p>Apps that want raw drag callbacks (e.g. drag-to-reposition) should attach an {@link
 * OnTouchListener} directly and observe {@link MotionEvent#ACTION_MOVE} — this class only exposes
 * the higher-level tap/long-press/fling vocabulary.
 *
 * <p>v1 caveat: multi-touch is not supported.
 */
public class GestureDetector implements OnTouchListener {
  /** Max DOWN→UP displacement (in pixels) for an event to count as a tap rather than a fling. */
  public static final int TAP_SLOP_PX = 12;

  /** Min DOWN→UP displacement (in pixels) for an event to count as a fling. */
  public static final int FLING_MIN_PX = 24;

  public interface OnGestureListener {
    /** Brief DOWN→UP within {@link #TAP_SLOP_PX} that wasn't preceded by a long-press. */
    void onSingleTap(MotionEvent e);

    /**
     * Called *during* a press when LVGL detects a long press (default ~400 ms hold). The follow-up
     * ACTION_UP will not call {@code onSingleTap}.
     */
    void onLongPress(MotionEvent e);

    /**
     * Called on UP when displacement >= {@link #FLING_MIN_PX}. Velocities are in pixels/second.
     * Sign matches axis direction (positive vx = rightward, positive vy = downward).
     */
    void onFling(MotionEvent down, MotionEvent up, float velocityX, float velocityY);
  }

  private final OnGestureListener listener;

  // State carried between DOWN and UP — single touch only in v1.
  private MotionEvent downEvent;
  private MotionEvent lastSampleEvent; // most recent DOWN or MOVE; used for lift-off velocity
  private boolean longPressFired;

  public GestureDetector(OnGestureListener listener) {
    this.listener = listener;
  }

  @Override
  public boolean onTouch(View v, MotionEvent event) {
    int action = event.getAction();
    if (action == MotionEvent.ACTION_DOWN) {
      downEvent = event;
      lastSampleEvent = event;
      longPressFired = false;
      return true;
    }
    if (action == MotionEvent.ACTION_MOVE) {
      lastSampleEvent = event;
      return true;
    }
    if (action == MotionEvent.ACTION_LONG_PRESS) {
      longPressFired = true;
      if (listener != null) {
        listener.onLongPress(event);
      }
      return true;
    }
    if (action == MotionEvent.ACTION_UP) {
      MotionEvent down = downEvent;
      MotionEvent slopeStart = lastSampleEvent != null ? lastSampleEvent : down;
      downEvent = null;
      lastSampleEvent = null;
      if (down == null) {
        return false; // UP without prior DOWN — touch likely cancelled
      }
      int dx = event.getX() - down.getX();
      int dy = event.getY() - down.getY();
      int absDx = dx < 0 ? -dx : dx;
      int absDy = dy < 0 ? -dy : dy;
      // Approximate distance with Chebyshev — exact pixel distance would
      // need a sqrt, and the slop / fling thresholds work fine on the
      // dominant axis alone for the gesture vocabulary we expose.
      int chebyshev = absDx > absDy ? absDx : absDy;

      if (longPressFired) {
        // Long press already fired the gesture; UP is just the cleanup.
        return true;
      }
      if (chebyshev <= TAP_SLOP_PX) {
        if (listener != null) {
          listener.onSingleTap(event);
        }
        return true;
      }
      if (chebyshev >= FLING_MIN_PX) {
        // Velocity uses the lift-off slope (last MOVE sample → UP) so a
        // flick whose speed comes from the final frames reads as fast.
        // Tap / fling classification (above) still uses DOWN→UP so a
        // slow drift can't be reclassified as a fling on a fast lift.
        int sdx = event.getX() - slopeStart.getX();
        int sdy = event.getY() - slopeStart.getY();
        long durationMs = event.getEventTime() - slopeStart.getEventTime();
        if (durationMs <= 0) {
          durationMs = 1; // avoid div-by-zero on freakishly fast events
        }
        float vx = (float) sdx * 1000.0f / (float) durationMs;
        float vy = (float) sdy * 1000.0f / (float) durationMs;
        if (listener != null) {
          listener.onFling(down, event, vx, vy);
        }
      }
      return true;
    }
    return false;
  }
}
