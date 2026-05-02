// SPDX-License-Identifier: GPL-3.0-only
package pickerdemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class PickerDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(PickerDemoActivity.class));
  }
}
