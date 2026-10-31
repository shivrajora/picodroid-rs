// SPDX-License-Identifier: GPL-3.0-only
package dialogdemo;

import picodroid.app.Application;

public class DialogDemoApp extends Application {
  @Override
  public void onCreate() {
    new DialogAppComponent();
  }
}
