// SPDX-License-Identifier: GPL-3.0-only
package animdemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class AnimDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(AnimDemoActivity.class));
  }
}
