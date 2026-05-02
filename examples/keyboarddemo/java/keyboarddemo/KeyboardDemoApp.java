// SPDX-License-Identifier: GPL-3.0-only
package keyboarddemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class KeyboardDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(KeyboardDemoActivity.class));
  }
}
