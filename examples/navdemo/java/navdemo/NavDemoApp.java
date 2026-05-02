// SPDX-License-Identifier: GPL-3.0-only
package navdemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class NavDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(HomeActivity.class));
  }
}
