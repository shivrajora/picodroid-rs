// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

/**
 * A widget for selecting a number from a range. Mirrors {@code android.widget.NumberPicker}'s API
 * subset: {@link #setMinValue}/{@link #setMaxValue} bound the range, {@link #setValue}/{@link
 * #getValue} access the current value (clamped to the range, never notifying the listener), and the
 * {@link OnValueChangeListener} fires once per value change.
 *
 * <p>Android renders a touch scroll-wheel; picodroid renders the current value in a focusable box.
 * On keypad boards, ENTER on the focused picker enters edit mode (secondary-color outline),
 * PREV/NEXT step the value by {@link #setStep step} while focus navigation is suspended, and ENTER
 * or BACK leaves edit mode.
 */
public class NumberPicker extends View {

  /** Value-change callback. Mirrors {@code android.widget.NumberPicker.OnValueChangeListener}. */
  public interface OnValueChangeListener {
    void onValueChange(NumberPicker picker, int oldVal, int newVal);
  }

  private OnValueChangeListener onValueChangeListener;
  private int minValue;
  private int maxValue;
  private int value;
  private int step = 1;

  public NumberPicker() {
    super(nativeCreate());
    nativeRegisterPicker();
    updateLabel();
  }

  public NumberPicker(Context ctx) {
    super(nativeCreate());
    nativeRegisterPicker();
    updateLabel();
  }

  private static native int nativeCreate();

  private native void nativeRegisterPicker();

  private native void nativeSetText(String text);

  /** Set the lower bound of the range, raising the current value into range if needed. */
  public void setMinValue(int min) {
    minValue = min;
    if (value < min) {
      value = min;
      updateLabel();
    }
  }

  public int getMinValue() {
    return minValue;
  }

  /** Set the upper bound of the range, lowering the current value into range if needed. */
  public void setMaxValue(int max) {
    maxValue = max;
    if (value > max) {
      value = max;
      updateLabel();
    }
  }

  public int getMaxValue() {
    return maxValue;
  }

  /**
   * Set the current value, clamped to [{@link #getMinValue()}, {@link #getMaxValue()}]. Does not
   * notify the {@link OnValueChangeListener}, matching Android.
   */
  public void setValue(int v) {
    value = clamp(v);
    updateLabel();
  }

  public int getValue() {
    return value;
  }

  /**
   * Picodroid extension: the value delta applied per edit-mode step. Android's NumberPicker steps
   * through consecutive values; on a four-button device a configurable step keeps wide ranges (e.g.
   * 0–10000 lux) usable. Values below 1 are treated as 1.
   */
  public void setStep(int s) {
    step = s < 1 ? 1 : s;
  }

  public int getStep() {
    return step;
  }

  public void setOnValueChangedListener(OnValueChangeListener listener) {
    this.onValueChangeListener = listener;
  }

  /**
   * Native dispatch target: apply one keypad edit-mode step. {@code direction} is +1 (PREV/A) or -1
   * (NEXT/B); the result is clamped, so steps at the range edge are silently absorbed.
   */
  void fireStep(int direction) {
    int newVal = clamp(value + direction * step);
    if (newVal == value) {
      return;
    }
    int oldVal = value;
    value = newVal;
    updateLabel();
    if (onValueChangeListener != null) {
      onValueChangeListener.onValueChange(this, oldVal, newVal);
    }
  }

  private int clamp(int v) {
    if (v < minValue) {
      return minValue;
    }
    if (v > maxValue) {
      return maxValue;
    }
    return v;
  }

  private void updateLabel() {
    nativeSetText(Integer.toString(value));
  }
}
