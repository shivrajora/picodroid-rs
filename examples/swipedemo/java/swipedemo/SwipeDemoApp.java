// SPDX-License-Identifier: GPL-3.0-only
package swipedemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class SwipeDemoApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(SwipeDemoActivity.class));
  }
}
