// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

/**
 * Mirrors {@code android.widget.RadioButton}: a two-state button with a circular indicator. Mutual
 * exclusion is provided by placing RadioButtons inside a {@link RadioGroup} — a standalone
 * RadioButton toggles like a CheckBox, exactly as on Android.
 */
public class RadioButton extends CompoundButton {
  public RadioButton() {
    super(nativeCreate());
  }

  public RadioButton(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void setText(String text);

  /** The group this button was added to, or {@code null}; set by {@link RadioGroup#addView}. */
  RadioGroup mGroup;

  @Override
  public native boolean isChecked();

  /**
   * Checks or unchecks the button. As on Android, checking a button that belongs to a {@link
   * RadioGroup} unchecks the group's previous selection and moves its checked id.
   */
  @Override
  public void setChecked(boolean checked) {
    nativeSetChecked(checked);
    if (mGroup != null) {
      mGroup.onButtonChecked(this, checked);
    }
  }

  /** The widget state alone; the group drives this when it is the one changing the selection. */
  void setCheckedSilently(boolean checked) {
    nativeSetChecked(checked);
  }

  private native void nativeSetChecked(boolean checked);
}
