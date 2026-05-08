// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

import picodroid.graphics.drawable.Drawable;

public class View {
  public static final int VISIBLE = 0;
  public static final int INVISIBLE = 1;
  public static final int GONE = 2;

  /**
   * Mirrors Android's {@code ViewGroup.LayoutParams.WRAP_CONTENT}. Passed to {@link #setSize}, the
   * view sizes itself to fit its children. Maps to LVGL's {@code LV_SIZE_CONTENT} at the FFI
   * boundary.
   */
  public static final int WRAP_CONTENT = -2;

  /** Swipe-direction constants matching LVGL's {@code lv_dir_t}. */
  public static final int SWIPE_LEFT = 1;

  public static final int SWIPE_RIGHT = 2;
  public static final int SWIPE_UP = 4;
  public static final int SWIPE_DOWN = 8;

  int nativeHandle;
  OnKeyListener onKeyListener;
  OnTouchListener onTouchListener;
  OnSwipeListener onSwipeListener;
  OnClickListener onClickListener;
  ViewGroup.LayoutParams layoutParams;

  protected View(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  /**
   * Click callback. Mirrors {@code android.view.View.OnClickListener} — fires after a finger
   * DOWN→UP gesture stays within the click slop and the widget is enabled. Any view that has a
   * click listener attached automatically becomes clickable.
   */
  public interface OnClickListener {
    void onClick(View v);
  }

  /**
   * Register a click listener. Setting a non-null listener flips this View's LVGL CLICKABLE flag so
   * touches generate {@code LV_EVENT_CLICKED}. Pass {@code null} to clear.
   */
  public void setOnClickListener(OnClickListener listener) {
    this.onClickListener = listener;
    nativeRegisterClickListener();
  }

  public void setOnKeyListener(OnKeyListener listener) {
    this.onKeyListener = listener;
    nativeRegisterKeyListener();
  }

  /**
   * Apply a {@link Drawable} as this view's background — used for rounded corners, gradients, and
   * stroke outlines. The drawable is dispatched virtually so subclasses (e.g. a future {@code
   * StateListDrawable}) can swap their fill on press/focus without changing the call site.
   */
  public void setBackground(Drawable drawable) {
    if (drawable != null) {
      drawable.applyTo(this);
    }
  }

  /**
   * Register a touch listener. The framework also flips this View's LVGL CLICKABLE flag so the
   * underlying touch indev routes Press/Release events here. Pass {@code null} to clear (the
   * CLICKABLE flag stays on — clearing it on a button widget would break click behavior).
   */
  public void setOnTouchListener(OnTouchListener listener) {
    this.onTouchListener = listener;
    nativeRegisterTouchListener();
  }

  /**
   * Register a swipe-gesture listener on this view. Fires once per gesture with one of {@link
   * #SWIPE_LEFT}, {@link #SWIPE_RIGHT}, {@link #SWIPE_UP}, {@link #SWIPE_DOWN}. The values mirror
   * LVGL's {@code lv_dir_t} bits — {@code SWIPE_UP=4} corresponds to a {@code LV_DIR_TOP} gesture
   * (finger moved upward).
   */
  public void setOnSwipeListener(OnSwipeListener listener) {
    this.onSwipeListener = listener;
    nativeRegisterSwipeListener();
  }

  private native void nativeRegisterClickListener();

  private native void nativeRegisterKeyListener();

  private native void nativeRegisterTouchListener();

  private native void nativeRegisterSwipeListener();

  void fireClick() {
    if (onClickListener != null) {
      onClickListener.onClick(this);
    }
  }

  boolean fireKey(KeyEvent event) {
    if (onKeyListener != null) {
      return onKeyListener.onKey(this, event);
    }
    return false;
  }

  boolean fireTouch(MotionEvent event) {
    if (onTouchListener != null) {
      return onTouchListener.onTouch(this, event);
    }
    return false;
  }

  void fireSwipe(int direction) {
    if (onSwipeListener != null) {
      onSwipeListener.onSwipe(this, direction);
    }
  }

  public native void setPosition(int x, int y);

  public native void setSize(int width, int height);

  public native void setBackgroundColor(int argb);

  public native void setVisibility(int visibility);

  public native void setPadding(int left, int top, int right, int bottom);

  public native void setEnabled(boolean enabled);

  public native void setAlpha(float alpha);

  public native void close();

  /**
   * Records the {@link ViewGroup.LayoutParams} that the parent layout should apply to this child.
   * The framework reads {@code width}/{@code height} during {@link ViewGroup#addView(View,
   * ViewGroup.LayoutParams)} and forwards them to {@link #setSize}; subclass-specific fields like
   * {@code LinearLayout.LayoutParams.weight} are applied by the parent layout itself.
   */
  public void setLayoutParams(ViewGroup.LayoutParams params) {
    this.layoutParams = params;
  }

  public ViewGroup.LayoutParams getLayoutParams() {
    return layoutParams;
  }

  /**
   * Apply a flex-grow weight to this view inside its {@link LinearLayout} parent. Visible to
   * picodroid.widget for {@link ViewGroup#addView(View, ViewGroup.LayoutParams)}'s weight handling.
   */
  native void nativeSetFlexGrow(int weight);

  /**
   * Synthesize a click event. Equivalent to {@code android.view.View#performClick()} — invokes the
   * registered {@link OnClickListener} without requiring a real touch. Useful for scripted UI
   * flows, accessibility, and headless end-to-end tests.
   */
  public native void performClick();

  /**
   * Returns a fresh {@link ViewPropertyAnimator} for this view. Mirrors {@code View.animate()} in
   * Android — chain alpha/x/y + setDuration on the result and call {@code start()}.
   */
  public ViewPropertyAnimator animate() {
    return new ViewPropertyAnimator(this);
  }
}
