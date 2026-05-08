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

  protected CompoundButton(int nativeHandle) {
    super(nativeHandle);
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

  void fireCheckedChanged() {
    if (onCheckedChangeListener != null) {
      onCheckedChangeListener.onCheckedChanged(this, isChecked());
    }
  }

  /** Mirrors {@code android.widget.CompoundButton.OnCheckedChangeListener}. */
  public interface OnCheckedChangeListener {
    void onCheckedChanged(CompoundButton buttonView, boolean isChecked);
  }
}
