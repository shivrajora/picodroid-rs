// SPDX-License-Identifier: GPL-3.0-only
package keydemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class KeyDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(KeyDemoActivity.class));
  }
}
