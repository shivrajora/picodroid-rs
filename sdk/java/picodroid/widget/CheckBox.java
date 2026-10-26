// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

public class CheckBox extends CompoundButton {
  public CheckBox(Context ctx) {
    super(ctx, nativeCreate());
  }

  private static native int nativeCreate();

  public native void setText(String text);

  @Override
  public native boolean isChecked();

  @Override
  public native void setChecked(boolean checked);
}
