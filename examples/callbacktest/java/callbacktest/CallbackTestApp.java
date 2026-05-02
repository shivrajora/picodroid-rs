// SPDX-License-Identifier: GPL-3.0-only
package callbacktest;

import picodroid.app.Application;
import picodroid.content.Intent;

public class CallbackTestApp extends Application {
  public void onCreate() {
    startActivity(new Intent(CallbackTestActivity.class));
  }
}
