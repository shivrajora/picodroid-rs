// SPDX-License-Identifier: GPL-3.0-only
package ellipsizedemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class EllipsizeDemoApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(EllipsizeDemoActivity.class));
  }
}
