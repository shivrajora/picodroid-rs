package picodroid.widget;

import picodroid.view.View;

/**
 * On-screen soft keyboard, wrapping LVGL's {@code lv_keyboard}. Keys type directly into the bound
 * {@link EditText} via LVGL — no Java→native round-trip per keystroke.
 *
 * <p>Two ways to use:
 *
 * <ul>
 *   <li><b>System keyboard (default):</b> just create any {@link EditText} — tapping it pops up a
 *       singleton system keyboard at the screen bottom. Dismissed by BACK or the keyboard's OK key.
 *       Apps that want this behavior do nothing extra.
 *   <li><b>Explicit instance:</b> for full control of placement and styling, {@code new
 *       Keyboard()}, then {@link #setEditText}, position via the inherited {@link
 *       View#setPosition}/{@link View#setSize}, and toggle visibility via {@link #show}/{@link
 *       #hide}. Pair with {@link EditText#setShowKeyboardOnTouch}{@code (false)} to disable the
 *       system keyboard for that field.
 * </ul>
 *
 * <p>v1 caveats: US-English layout only (LVGL's default), no slide-up animation by default (chain
 * {@link View#animate} yourself), no {@code OnEditorActionListener} on EditText (use {@link
 * #setOnReadyListener} on an explicit instance for done-detection).
 */
public class Keyboard extends View {
  public static final int MODE_TEXT_LOWER = 0;
  public static final int MODE_TEXT_UPPER = 1;
  public static final int MODE_SPECIAL = 2;
  public static final int MODE_NUMBER = 3;

  private Runnable onReadyListener;

  public Keyboard() {
    super(nativeCreate());
  }

  /** Bind this keyboard to type into the given {@link EditText}. */
  public void setEditText(EditText editText) {
    nativeSetTextarea(editText);
  }

  /**
   * Initial mode — defaults to {@link #MODE_TEXT_LOWER}. LVGL switches modes internally as the user
   * taps the keyboard's "abc"/"ABC"/"123"/"!@#" toggle keys.
   */
  public void setMode(int mode) {
    nativeSetMode(mode);
  }

  public void show() {
    setVisibility(VISIBLE);
  }

  public void hide() {
    setVisibility(GONE);
  }

  /**
   * Fired when the user taps the keyboard's OK / Enter key. The keyboard does not auto-hide on
   * READY for explicit instances — apps decide what done means (validate input, dismiss, advance
   * focus, etc.). Use {@link #hide} from inside the listener to dismiss.
   */
  public void setOnReadyListener(Runnable listener) {
    this.onReadyListener = listener;
    nativeRegisterReadyListener();
  }

  void fireReady() {
    if (onReadyListener != null) {
      onReadyListener.run();
    }
  }

  private static native int nativeCreate();

  /**
   * Instance method; reads {@code this.nativeHandle} (the keyboard) on the native side and the
   * EditText's handle via the View field-slot table — same idiom GradientDrawable.nativeApply uses
   * to stay outside the picodroid.view package where {@code nativeHandle} is package-private.
   */
  private native void nativeSetTextarea(EditText target);

  private native void nativeSetMode(int mode);

  private native void nativeRegisterReadyListener();
}
