// SPDX-License-Identifier: GPL-3.0-only
package dialogdemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class DialogDemoApp extends Application {
  public void onCreate() {
    new DialogAppComponent();
    startActivity(new Intent(DialogDemoActivity.class));
  }
}
