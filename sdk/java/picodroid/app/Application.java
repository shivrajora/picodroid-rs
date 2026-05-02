// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.content.Intent;

public class Application extends Context {
  public void onCreate() {
    // Subclass overrides this
  }

  public native void startActivity(Intent intent);
}
