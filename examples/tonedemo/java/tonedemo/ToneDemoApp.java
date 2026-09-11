// SPDX-License-Identifier: GPL-3.0-only
package tonedemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class ToneDemoApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(ToneDemoActivity.class));
  }
}
