// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

/**
 * Shared superclass for two-state widgets ({@link CheckBox}, {@link Switch}, {@link ToggleButton})
 * that mirror Android's {@code android.widget.CompoundButton}. Holds the typed checked-change
 * listener and the package-private {@code fireCheckedChanged} dispatch hook called from the native
 * event loop.
 */
public abstract class CompoundButton extends View {
  OnCheckedChangeListener onCheckedChangeListener;

  /**
   * The framework's own listener (a {@link RadioGroup} tracking its buttons), kept apart from the
   * app's so neither displaces the other; mirrors Android's {@code
   * setOnCheckedChangeWidgetListener}.
   */
  OnCheckedChangeListener mWidgetListener;

  protected CompoundButton(int nativeHandle) {
    super(nativeHandle);
    // Focusable by default, as on Android (see Button).
    setFocusable(true);
  }

  /** Returns whether this widget is currently in the checked state. */
  public abstract boolean isChecked();

  /** Programmatically set the checked state; does not fire the listener. */
  public abstract void setChecked(boolean checked);

  /**
   * Register a listener invoked whenever the checked state changes via user interaction or {@link
   * #performCheckedChange()}. Pass {@code null} to clear.
   */
  public void setOnCheckedChangeListener(OnCheckedChangeListener listener) {
    this.onCheckedChangeListener = listener;
    nativeRegisterCheckedChangeListener();
  }

  /**
   * Synthesize a checked-change event for headless testing. Toggles the underlying state and fires
   * the registered listener on the next main-loop dispatch tick.
   */
  public native void performCheckedChange();

  protected native void nativeRegisterCheckedChangeListener();

  void setOnCheckedChangeWidgetListener(OnCheckedChangeListener listener) {
    this.mWidgetListener = listener;
    nativeRegisterCheckedChangeListener();
  }

  void fireCheckedChanged() {
    boolean checked = isChecked();
    if (mWidgetListener != null) {
      mWidgetListener.onCheckedChanged(this, checked);
    }
    if (onCheckedChangeListener != null) {
      onCheckedChangeListener.onCheckedChanged(this, checked);
    }
  }

  /** Mirrors {@code android.widget.CompoundButton.OnCheckedChangeListener}. */
  public interface OnCheckedChangeListener {
    void onCheckedChanged(CompoundButton buttonView, boolean isChecked);
  }
}
