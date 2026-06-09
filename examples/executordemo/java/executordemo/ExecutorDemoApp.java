// SPDX-License-Identifier: GPL-3.0-only
package executordemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class ExecutorDemoApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(ExecutorDemoActivity.class));
  }
}
