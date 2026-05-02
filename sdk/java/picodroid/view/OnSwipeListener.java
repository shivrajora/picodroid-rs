// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Callback for swipe-gesture events on a {@link View}. {@code direction} is one of {@link
 * View#SWIPE_LEFT}, {@link View#SWIPE_RIGHT}, {@link View#SWIPE_UP}, {@link View#SWIPE_DOWN}.
 */
public interface OnSwipeListener {
  void onSwipe(View view, int direction);
}
