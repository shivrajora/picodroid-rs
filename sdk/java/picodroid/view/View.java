// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

import picodroid.concurrent.Executor;
import picodroid.concurrent.Executors;
import picodroid.content.Context;
import picodroid.content.res.ColorStateList;
import picodroid.graphics.Canvas;
import picodroid.graphics.drawable.Drawable;

public class View {
  /** This view is visible. Matches Android's value. */
  public static final int VISIBLE = 0x00000000;

  /** This view is invisible but still takes up layout space. Matches Android's value. */
  public static final int INVISIBLE = 0x00000004;

  /** This view is invisible and takes no layout space. Matches Android's value. */
  public static final int GONE = 0x00000008;

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

  /**
   * The {@link ViewGroup} this view was {@link ViewGroup#addView added} to, or {@code null}: what
   * {@link #getParent} returns, and how {@link #close} finds the child list it has to leave.
   */
  ViewGroup mParent;

  OnKeyListener onKeyListener;
  OnTouchListener onTouchListener;
  OnSwipeListener onSwipeListener;
  OnClickListener onClickListener;
  OnLongClickListener onLongClickListener;
  OnFocusChangeListener onFocusChangeListener;
  ViewGroup.LayoutParams layoutParams;
  boolean focusable = false; // Android default for a plain View / ViewGroup.

  // App-set state cached for the getters, mirroring Android (where these live
  // in View's own flag/property fields, not the renderer): the framework
  // hiding a covered Activity's tree does NOT change a child's visibility.
  int visibility = VISIBLE;
  boolean enabled = true;
  float alpha = 1.0f;

  /** Android's "no ID" sentinel, {@code android.view.View#NO_ID}. */
  public static final int NO_ID = -1;

  int id = NO_ID;
  Object tag;

  /** What {@link #setBackground} was last given; null for none or a plain colour. */
  private Drawable mBackground;

  /** The tint set by {@link #setBackgroundTintList}, or null. */
  private ColorStateList mBackgroundTint;

  /**
   * The canvas {@link #onDraw} draws on; non-null only for a view made with {@link #View(Context)},
   * the one kind whose widget keeps a drawing. The framework widgets draw themselves.
   */
  private Canvas mCanvas;

  /** An {@link #onDraw} is queued on the main thread and has not run yet. */
  private boolean mDrawPending;

  /** The queued task that runs {@link #onDraw}; made on the first {@link #invalidate}. */
  private Runnable mDrawTask;

  /** The main thread's executor, shared by every view that draws. */
  private static Executor sMainExecutor;

  protected View(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  /**
   * A plain view that draws what its {@link #onDraw} draws: subclass it and override {@code
   * onDraw}, as on Android. It starts transparent, borderless and not clickable, 0 by 0 until it is
   * sized ({@link #setSize}, or a layout); its first {@code onDraw} runs on the main thread shortly
   * after construction.
   */
  public View(Context context) {
    this(nativeCreateView());
    mCanvas = new Canvas();
    scheduleDraw();
  }

  private static native int nativeCreateView();

  /**
   * Draws this view. Mirrors {@code android.view.View#onDraw(Canvas)}: runs on the main thread
   * after {@link #invalidate}, and what it draws is kept and repainted until the next call, so it
   * must draw the whole view each time. Only a view made with {@link #View(Context)} is drawn this
   * way; the default draws nothing.
   */
  protected void onDraw(Canvas canvas) {}

  /**
   * Asks for {@link #onDraw} to run again, on the main thread before the next frame. Mirrors {@code
   * android.view.View#invalidate()}: several calls before it runs cost one {@code onDraw}. Call it
   * when the state {@code onDraw} reads changes, and after a size change. Framework widgets repaint
   * themselves when their state changes, so on them this does nothing.
   */
  public void invalidate() {
    if (mCanvas != null) {
      scheduleDraw();
    }
  }

  /** {@link #invalidate} from any thread. Mirrors {@code android.view.View#postInvalidate()}. */
  public void postInvalidate() {
    invalidate();
  }

  private void scheduleDraw() {
    if (mDrawPending || nativeHandle == 0) {
      return;
    }
    mDrawPending = true;
    if (mDrawTask == null) {
      mDrawTask = new DrawTask(this);
    }
    Executor main = sMainExecutor;
    if (main == null) {
      main = Executors.mainExecutor();
      sMainExecutor = main;
    }
    main.execute(mDrawTask);
  }

  /** Runs {@link #onDraw} into this view's recording, then has the view repainted. */
  final void performDraw() {
    mDrawPending = false;
    Canvas canvas = mCanvas;
    if (canvas == null || nativeHandle == 0) {
      return;
    }
    nativeBeginDraw(canvas);
    try {
      onDraw(canvas);
    } finally {
      nativeEndDraw(canvas);
    }
  }

  /** Points {@code canvas} at this view's widget and size and empties the old recording. */
  private native void nativeBeginDraw(Canvas canvas);

  /** Detaches {@code canvas} again and repaints the view. */
  private native void nativeEndDraw(Canvas canvas);

  /** The main-thread task behind {@link #invalidate}; one per drawing view, reused. */
  private static final class DrawTask implements Runnable {
    private final View view;

    DrawTask(View view) {
      this.view = view;
    }

    @Override
    public void run() {
      view.performDraw();
    }
  }

  /**
   * Mirrors {@code android.view.View.MeasureSpec}: a size and how binding it is, packed in an int,
   * as {@link #onMeasure} receives them.
   */
  public static class MeasureSpec {
    private static final int MODE_SHIFT = 30;
    private static final int MODE_MASK = 0x3 << MODE_SHIFT;

    /** The parent sets no limit: the view may be any size it wants. */
    public static final int UNSPECIFIED = 0 << MODE_SHIFT;

    /** The parent has decided the size: the view gets exactly that. */
    public static final int EXACTLY = 1 << MODE_SHIFT;

    /** The view may be as large as it wants, up to the size given. */
    public static final int AT_MOST = 2 << MODE_SHIFT;

    // What the onMeasure now running reported; -1 until it calls setMeasuredDimension. Measuring
    // happens on the main thread, one view at a time, so one pair serves every view. It lives
    // here and not on View: a class's static fields take a slot in each of its instances, and
    // nobody makes a MeasureSpec.
    static int sWidth = -1;
    static int sHeight;

    private MeasureSpec() {}

    public static int makeMeasureSpec(int size, int mode) {
      return (size & ~MODE_MASK) | (mode & MODE_MASK);
    }

    public static int getMode(int measureSpec) {
      return measureSpec & MODE_MASK;
    }

    public static int getSize(int measureSpec) {
      return measureSpec & ~MODE_MASK;
    }
  }

  /**
   * Mirrors {@code android.view.View#onMeasure(int, int)}: decide how large this view wants to be,
   * within what the two {@link MeasureSpec}s allow, and say so with {@link #setMeasuredDimension}.
   * The default takes the size the spec gives, or for an {@code UNSPECIFIED} one the size the view
   * already has.
   *
   * <p>Called for a view that draws itself (one made with {@link #View(Context)}), as it is added
   * to a layout with a {@code wrap_content} dimension: the framework's widgets are sized by the
   * renderer, which knows their content, and a fixed or {@code match_parent} size needs no asking.
   * A {@code wrap_content} dimension is measured {@code UNSPECIFIED}; the other, when it is fixed,
   * {@code EXACTLY}.
   */
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    setMeasuredDimension(
        getDefaultSize(getSuggestedMinimumWidth(), widthMeasureSpec),
        getDefaultSize(getSuggestedMinimumHeight(), heightMeasureSpec));
  }

  /** Mirrors Android: what {@link #onMeasure} must call with the size it decided on. */
  protected final void setMeasuredDimension(int measuredWidth, int measuredHeight) {
    MeasureSpec.sWidth = measuredWidth < 0 ? 0 : measuredWidth;
    MeasureSpec.sHeight = measuredHeight < 0 ? 0 : measuredHeight;
  }

  /**
   * Mirrors {@code android.view.View#measure(int, int)}: runs {@link #onMeasure} and takes the size
   * it reports, which {@link #getMeasuredWidth} then returns.
   */
  public final void measure(int widthMeasureSpec, int heightMeasureSpec) {
    MeasureSpec.sWidth = -1;
    onMeasure(widthMeasureSpec, heightMeasureSpec);
    if (MeasureSpec.sWidth < 0) {
      throw new IllegalStateException(
          "View with id "
              + getId()
              + ": onMeasure() did not set the measured dimension by calling"
              + " setMeasuredDimension()");
    }
    setSize(MeasureSpec.sWidth, MeasureSpec.sHeight);
    invalidate();
  }

  /** Mirrors Android: the width {@link #onMeasure} last settled on; the laid-out width here. */
  public final int getMeasuredWidth() {
    return getWidth();
  }

  /** Mirrors Android: the height {@link #onMeasure} last settled on; the laid-out height. */
  public final int getMeasuredHeight() {
    return getHeight();
  }

  /** Mirrors Android: {@code size} unless the spec says otherwise (EXACTLY or AT_MOST). */
  public static int getDefaultSize(int size, int measureSpec) {
    return MeasureSpec.getMode(measureSpec) == MeasureSpec.UNSPECIFIED
        ? size
        : MeasureSpec.getSize(measureSpec);
  }

  /** Mirrors Android: {@code size} if the spec allows it, else what the spec imposes. */
  public static int resolveSize(int size, int measureSpec) {
    int mode = MeasureSpec.getMode(measureSpec);
    int specSize = MeasureSpec.getSize(measureSpec);
    if (mode == MeasureSpec.EXACTLY) {
      return specSize;
    }
    if (mode == MeasureSpec.AT_MOST) {
      return size < specSize ? size : specSize;
    }
    return size;
  }

  /** The width an unconstrained default measure settles on: the one the view has now. */
  protected int getSuggestedMinimumWidth() {
    return getWidth();
  }

  /** The height an unconstrained default measure settles on: the one the view has now. */
  protected int getSuggestedMinimumHeight() {
    return getHeight();
  }

  /**
   * Takes the size a parent's LayoutParams give: a view that draws itself is asked how large it
   * wants to be for a {@code wrap_content} dimension, every other size goes to the renderer.
   */
  final void applyLayoutSize(int width, int height) {
    if (mCanvas != null && (width == WRAP_CONTENT || height == WRAP_CONTENT)) {
      measure(measureSpecFor(width), measureSpecFor(height));
      if (width != WRAP_CONTENT || height != WRAP_CONTENT) {
        // The measured value stands for the wrapped dimension; the other is the layout's.
        setSize(
            width == WRAP_CONTENT ? MeasureSpec.sWidth : width,
            height == WRAP_CONTENT ? MeasureSpec.sHeight : height);
      }
    } else {
      setSize(width, height);
    }
  }

  private static int measureSpecFor(int size) {
    return size >= 0
        ? MeasureSpec.makeMeasureSpec(size, MeasureSpec.EXACTLY)
        : MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED);
  }

  /** Whether this view's widget has been freed ({@link ViewGroup#removeView}). */
  final boolean isReleased() {
    return nativeHandle == 0;
  }

  /**
   * Forgets the freed widget. A released view holds handle 0, which every native arm refuses with
   * {@code IllegalStateException} — where a stale handle was a dangling widget pointer on the
   * boards without a generational handle table (the touch kit hung in the QA app's tree section).
   */
  void release() {
    nativeHandle = 0;
    mParent = null;
  }

  /**
   * The parent this view was {@link ViewGroup#addView added} to, or {@code null} when it has none:
   * never added, or {@link ViewGroup#removeView removed} since. Mirrors {@code
   * android.view.View#getParent()}; the one implementation is {@link ViewGroup}, so the Android
   * idiom {@code ((ViewGroup) v.getParent()).removeView(v)} works as written.
   */
  public final ViewParent getParent() {
    return mParent;
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
   * Long-click callback. Mirrors {@code android.view.View.OnLongClickListener} — fires when a press
   * is held past LVGL's long-press threshold (~400 ms). Returning {@code true} consumes the event;
   * a consumed long-click suppresses the subsequent click (see {@link #performLongClick()}).
   */
  public interface OnLongClickListener {
    boolean onLongClick(View v);
  }

  /**
   * Focus-change callback. Mirrors {@code android.view.View.OnFocusChangeListener} — fires when
   * this view gains or loses input focus (on a hardware-button device, as PREV/NEXT move the keypad
   * focus highlight between focusable views).
   */
  public interface OnFocusChangeListener {
    void onFocusChange(View v, boolean hasFocus);
  }

  /**
   * Register a click listener. Setting a non-null listener flips this View's LVGL CLICKABLE flag so
   * touches generate {@code LV_EVENT_CLICKED}. Pass {@code null} to clear.
   */
  public void setOnClickListener(OnClickListener listener) {
    this.onClickListener = listener;
    nativeRegisterClickListener();
  }

  /**
   * Register a long-click listener. Like {@link #setOnClickListener}, attaching one flips this
   * View's LVGL CLICKABLE flag so a held press generates {@code LV_EVENT_LONG_PRESSED}. Pass {@code
   * null} to clear (the native registration is idempotent).
   */
  public void setOnLongClickListener(OnLongClickListener listener) {
    this.onLongClickListener = listener;
    nativeRegisterLongClickListener();
  }

  public void setOnKeyListener(OnKeyListener listener) {
    this.onKeyListener = listener;
    nativeRegisterKeyListener();
  }

  /**
   * Set whether this view can take input focus. Mirrors {@code
   * android.view.View#setFocusable(boolean)}. On a hardware-button device, only a focusable view
   * receives key events — call this (and {@link #requestFocus()}) on the view that owns the {@link
   * OnKeyListener}. Focusability is independent of {@link #setOnKeyListener}, exactly as in
   * Android.
   */
  public void setFocusable(boolean focusable) {
    this.focusable = focusable;
    nativeSetFocusable(focusable);
  }

  /** Returns whether this view can take focus. Mirrors {@code android.view.View#isFocusable()}. */
  public boolean isFocusable() {
    return focusable;
  }

  /**
   * Request that this view take input focus. Mirrors {@code android.view.View#requestFocus()}:
   * returns {@code false} (without effect) if the view is not {@link #isFocusable() focusable},
   * otherwise {@code true} if it became the focused view.
   */
  public boolean requestFocus() {
    if (!focusable) {
      return false;
    }
    return nativeRequestFocus();
  }

  /**
   * Returns whether this view currently has input focus. Mirrors {@code
   * android.view.View#isFocused()} — true iff this view is the active keypad focus group's focused
   * widget.
   */
  public boolean isFocused() {
    return nativeIsFocused();
  }

  /**
   * Returns whether this view (or a descendant) has input focus. Mirrors {@code
   * android.view.View#hasFocus()}. Picodroid focuses leaf widgets directly, so this is equivalent
   * to {@link #isFocused()}.
   */
  public boolean hasFocus() {
    return nativeIsFocused();
  }

  /**
   * Register a focus-change listener. Mirrors {@code android.view.View#setOnFocusChangeListener}.
   * The view must also be {@link #setFocusable(boolean) focusable} (or an adapter row) to ever
   * receive focus and fire this callback.
   */
  public void setOnFocusChangeListener(OnFocusChangeListener listener) {
    this.onFocusChangeListener = listener;
    nativeRegisterFocusChangeListener();
  }

  /** Returns the registered focus-change listener, or {@code null}. */
  public OnFocusChangeListener getOnFocusChangeListener() {
    return onFocusChangeListener;
  }

  /**
   * Apply a {@link Drawable} as this view's background — used for rounded corners, gradients, and
   * stroke outlines. The drawable is dispatched virtually so subclasses (e.g. a future {@code
   * StateListDrawable}) can swap their fill on press/focus without changing the call site.
   */
  public void setBackground(Drawable drawable) {
    if (drawable == mBackground) {
      return; // the same instance again, as on Android: nothing to redo
    }
    mBackground = drawable;
    if (drawable != null) {
      drawable.applyTo(this);
      if (mBackgroundTint != null) {
        nativeSetBackgroundTint(mBackgroundTint.getDefaultColor());
      }
    }
  }

  /** Mirrors Android: the drawable last given to {@link #setBackground}, or {@code null}. */
  public Drawable getBackground() {
    return mBackground;
  }

  /**
   * Mirrors {@code android.view.View#setBackgroundTintList(ColorStateList)}: recolours the
   * background, keeping its shape (a {@link picodroid.graphics.drawable.GradientDrawable}'s corners
   * and stroke stay). The way to change the colour of a dot or a pill without building a new
   * drawable. A tint equal to the current one does nothing; {@code null} takes the tint off and
   * puts the drawable's own colour back.
   *
   * <p>Divergences: the tint's colour replaces the background's and its alpha is ignored (the
   * background keeps its own opacity, so a view with no background still shows none), and after a
   * {@code null} only a background set with {@link #setBackground} gets its colour back.
   */
  public void setBackgroundTintList(ColorStateList tint) {
    ColorStateList old = mBackgroundTint;
    if (tint == null
        ? old == null
        : old != null && old.getDefaultColor() == tint.getDefaultColor()) {
      return;
    }
    mBackgroundTint = tint;
    if (tint != null) {
      nativeSetBackgroundTint(tint.getDefaultColor());
    } else if (mBackground != null) {
      mBackground.applyTo(this);
    }
  }

  /** Mirrors Android: the tint set by {@link #setBackgroundTintList}, or {@code null}. */
  public ColorStateList getBackgroundTintList() {
    return mBackgroundTint;
  }

  private native void nativeSetBackgroundTint(int argb);

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
   * (finger moved upward). The listener hears swipes that start on this view or on any descendant
   * without a listener of its own; a scrollable ancestor that can scroll in the swipe's direction
   * takes the drag as a scroll first.
   */
  public void setOnSwipeListener(OnSwipeListener listener) {
    this.onSwipeListener = listener;
    nativeRegisterSwipeListener();
  }

  private native void nativeRegisterClickListener();

  private native void nativeRegisterLongClickListener();

  private native void nativeRegisterKeyListener();

  private native void nativeSetFocusable(boolean focusable);

  private native boolean nativeRequestFocus();

  private native boolean nativeIsFocused();

  private native void nativeRegisterFocusChangeListener();

  private native void nativeRegisterTouchListener();

  private native void nativeRegisterSwipeListener();

  void fireClick() {
    if (onClickListener != null) {
      onClickListener.onClick(this);
    }
  }

  /**
   * Invoked from the native event loop on a long press. Returns whether the listener consumed it
   * (false when none is set), mirroring {@code View.OnLongClickListener.onLongClick}'s contract.
   */
  boolean fireLongClick() {
    if (onLongClickListener != null) {
      return onLongClickListener.onLongClick(this);
    }
    return false;
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

  void fireFocusChange(boolean hasFocus) {
    if (onFocusChangeListener != null) {
      onFocusChangeListener.onFocusChange(this, hasFocus);
    }
  }

  public native void setPosition(int x, int y);

  public native void setSize(int width, int height);

  /**
   * Mirrors {@code android.view.View#setBackgroundColor(int)}: a plain fill in place of whatever
   * background the view had.
   */
  public void setBackgroundColor(int argb) {
    mBackground = null;
    nativeSetBackgroundColor(argb);
  }

  private native void nativeSetBackgroundColor(int argb);

  /**
   * Set visibility to one of {@link #VISIBLE}, {@link #INVISIBLE}, or {@link #GONE}. Setting the
   * visibility the view already has does nothing, as on Android.
   */
  public void setVisibility(int visibility) {
    if (visibility == this.visibility) {
      return;
    }
    this.visibility = visibility;
    nativeSetVisibility(visibility);
  }

  /** Returns the last app-set visibility. Mirrors {@code android.view.View#getVisibility()}. */
  public int getVisibility() {
    return visibility;
  }

  public native void setPadding(int left, int top, int right, int bottom);

  public void setEnabled(boolean enabled) {
    if (enabled == this.enabled) {
      return;
    }
    this.enabled = enabled;
    nativeSetEnabled(enabled);
  }

  /** Returns the enabled state set via {@link #setEnabled}. Mirrors Android. */
  public boolean isEnabled() {
    return enabled;
  }

  /** Sets the opacity, 0 to 1. Setting the alpha the view already has does nothing. */
  public void setAlpha(float alpha) {
    if (alpha == this.alpha) {
      return;
    }
    this.alpha = alpha;
    nativeSetAlpha(alpha);
  }

  /**
   * Returns the alpha set via {@link #setAlpha}, or the target of the last started {@link
   * ViewPropertyAnimator#alpha} animation. Mirrors {@code android.view.View#getAlpha()} at rest;
   * the per-frame interpolated value is not exposed.
   */
  public float getAlpha() {
    return alpha;
  }

  /**
   * Left edge of this view relative to its parent, in pixels, after layout and excluding {@link
   * #getTranslationX() translation}. Mirrors {@code android.view.View#getLeft()}.
   */
  public native int getLeft();

  /**
   * Top edge of this view relative to its parent, in pixels, after layout and excluding
   * translation. Mirrors Android.
   */
  public native int getTop();

  /** Laid-out width in pixels. Mirrors {@code android.view.View#getWidth()}. */
  public native int getWidth();

  /** Laid-out height in pixels. Mirrors {@code android.view.View#getHeight()}. */
  public native int getHeight();

  /**
   * Horizontal offset from the laid-out position, in pixels. Mirrors {@code
   * android.view.View#setTranslationX(float)}. Unlike {@link #setPosition}, translation also works
   * on children of a {@link picodroid.widget.LinearLayout}, which positions its children itself.
   */
  public void setTranslationX(float translationX) {
    nativeSetProperty(ViewPropertyAnimator.PROPERTY_TRANSLATION_X, translationX);
  }

  /** Returns the horizontal translation. Mirrors {@code android.view.View#getTranslationX()}. */
  public float getTranslationX() {
    return nativeGetProperty(ViewPropertyAnimator.PROPERTY_TRANSLATION_X);
  }

  /** Vertical offset from the laid-out position, in pixels. See {@link #setTranslationX}. */
  public void setTranslationY(float translationY) {
    nativeSetProperty(ViewPropertyAnimator.PROPERTY_TRANSLATION_Y, translationY);
  }

  /** Returns the vertical translation. Mirrors {@code android.view.View#getTranslationY()}. */
  public float getTranslationY() {
    return nativeGetProperty(ViewPropertyAnimator.PROPERTY_TRANSLATION_Y);
  }

  /**
   * Rotation about the view's centre, in degrees clockwise. Mirrors {@code
   * android.view.View#setRotation(float)}. A rotated or scaled view renders through an off-screen
   * layer of its own size — see {@link ViewPropertyAnimator} for the memory budget; keep
   * transformed views small.
   */
  public void setRotation(float rotation) {
    nativeSetProperty(ViewPropertyAnimator.PROPERTY_ROTATION, rotation);
  }

  /** Returns the rotation in degrees (0.1° resolution). Mirrors Android. */
  public float getRotation() {
    return nativeGetProperty(ViewPropertyAnimator.PROPERTY_ROTATION);
  }

  /**
   * Horizontal scale about the view's centre; {@code 1.0} is unscaled. Mirrors {@code
   * android.view.View#setScaleX(float)}. Negative values (Android's mirror) clamp to {@code 0}.
   */
  public void setScaleX(float scaleX) {
    nativeSetProperty(ViewPropertyAnimator.PROPERTY_SCALE_X, scaleX);
  }

  /** Returns the horizontal scale (1/256 resolution). Mirrors Android. */
  public float getScaleX() {
    return nativeGetProperty(ViewPropertyAnimator.PROPERTY_SCALE_X);
  }

  /** Vertical scale about the view's centre; {@code 1.0} is unscaled. See {@link #setScaleX}. */
  public void setScaleY(float scaleY) {
    nativeSetProperty(ViewPropertyAnimator.PROPERTY_SCALE_Y, scaleY);
  }

  /** Returns the vertical scale. Mirrors Android. */
  public float getScaleY() {
    return nativeGetProperty(ViewPropertyAnimator.PROPERTY_SCALE_Y);
  }

  /**
   * Visual horizontal position: {@code getLeft() + getTranslationX()}. Mirrors {@code
   * android.view.View#getX()}.
   */
  public float getX() {
    return getLeft() + getTranslationX();
  }

  /** Visual vertical position: {@code getTop() + getTranslationY()}. See {@link #getX()}. */
  public float getY() {
    return getTop() + getTranslationY();
  }

  /**
   * Set this view's identifier. Mirrors {@code android.view.View#setId(int)}. A layout's {@code
   * android:id="@+id/title"} calls this with the generated {@code R.id.title}.
   */
  public void setId(int id) {
    this.id = id;
  }

  /** Returns this view's identifier, or {@link #NO_ID}. Mirrors Android. */
  public int getId() {
    return id;
  }

  /**
   * Mirrors Android: this view if its id is {@code id}, else the first match among its descendants
   * (depth first), else {@code null}. {@link #NO_ID} never matches.
   */
  @SuppressWarnings("TypeParameterUnusedInFormals") // Android's signature, since API 26
  public final <T extends View> T findViewById(int id) {
    if (id == NO_ID) {
      return null;
    }
    @SuppressWarnings("unchecked")
    T found = (T) findViewTraversal(id);
    return found;
  }

  /** Mirrors Android's hook of the same name; {@link ViewGroup} extends the search to children. */
  protected View findViewTraversal(int id) {
    return id == this.id ? this : null;
  }

  /** Attach an arbitrary tag object. Mirrors {@code android.view.View#setTag(Object)}. */
  public void setTag(Object tag) {
    this.tag = tag;
  }

  /** Returns the tag set via {@link #setTag}, or {@code null}. Mirrors Android. */
  public Object getTag() {
    return tag;
  }

  /**
   * The animator was cancelled mid-flight: the alpha is wherever its last frame left it, so the
   * cached value {@link #setAlpha} compares against is read back from the renderer.
   */
  final void syncAlpha() {
    alpha = nativeGetProperty(ViewPropertyAnimator.PROPERTY_ALPHA);
  }

  private native void nativeSetVisibility(int visibility);

  private native void nativeSetEnabled(boolean enabled);

  private native void nativeSetAlpha(float alpha);

  /**
   * Transform accessors; {@code property} is a {@code ViewPropertyAnimator.PROPERTY_*} code, so the
   * setters and the animator share one native unit conversion.
   */
  private native void nativeSetProperty(int property, float value);

  private native float nativeGetProperty(int property);

  /**
   * Frees this view's widget. Not an Android method (Android's views are garbage collected), but an
   * embedded panel wants a subtree gone the moment the app is done with it. A parented view leaves
   * its parent first, exactly as {@link ViewGroup#removeView} would: the parent's child list must
   * not keep the freed subtree reachable (a dashboard once turned pages with {@code close()} alone
   * and ran out of heap a dozen turns later). Afterwards the view is released: every further native
   * call on it throws {@code IllegalStateException}, {@link ViewGroup#addView} refuses it, and a
   * second {@code close()} is a no-op.
   */
  public void close() {
    ViewGroup parent = mParent;
    if (parent != null) {
      parent.removeView(this);
      return;
    }
    if (!isReleased()) {
      nativeClose();
      release();
    }
  }

  /** Frees the widget; the Java side has already left any parent's child list. */
  private native void nativeClose();

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

  /** Space to keep clear around this view in a {@code LinearLayout}, in pixels. */
  native void nativeSetMargins(int left, int top, int right, int bottom);

  /**
   * Places this view in a {@code FrameLayout}: against the edges or centre {@code gravity} names,
   * moved by ({@code dx}, {@code dy}) pixels.
   */
  native void nativeSetFrameGravity(int gravity, int dx, int dy);

  /**
   * Synthesize a click event. Equivalent to {@code android.view.View#performClick()} — invokes the
   * registered {@link OnClickListener} without requiring a real touch. Useful for scripted UI
   * flows, accessibility, and headless end-to-end tests.
   */
  public native void performClick();

  /**
   * Synthesize a long-click. Mirrors {@code android.view.View#performLongClick()} — invokes the
   * registered {@link OnLongClickListener} directly (no input synthesis) and returns whether it
   * consumed the event. Pure Java: a long click is a listener call, so no native round-trip is
   * needed.
   */
  public boolean performLongClick() {
    return fireLongClick();
  }

  /** Synthesize an LVGL long-press event for scripted tests of the real-input path. */
  native void performLongClickNative();

  /**
   * Returns a fresh {@link ViewPropertyAnimator} for this view. Mirrors {@code View.animate()} in
   * Android — chain target values ({@code alpha}, {@code translationX}, {@code rotation}, {@code
   * scaleX} …) plus {@code setDuration}/{@code setStartDelay} on the result and call {@code
   * start()}.
   */
  public ViewPropertyAnimator animate() {
    return new ViewPropertyAnimator(this);
  }
}
