// SPDX-License-Identifier: GPL-3.0-only
package gesturedemo;

import picodroid.app.Application;

public class GestureDemoApp extends Application {
  @Override
  public void onCreate() {
    new GestureAppComponent();
  }
}
