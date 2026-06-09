// SPDX-License-Identifier: GPL-3.0-only
package sensordemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class SensorDemoApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(SensorDemoActivity.class));
  }
}
