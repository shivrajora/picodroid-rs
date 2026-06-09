// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

public class Switch extends CompoundButton {
  public Switch() {
    super(nativeCreate());
  }

  public Switch(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  @Override
  public native boolean isChecked();

  @Override
  public native void setChecked(boolean checked);

  public native void toggle();
}
