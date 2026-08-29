// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Inject;
import picodroid.app.Service;
import picodroid.content.Intent;
import picodroid.os.IBinder;
import picodroid.util.Log;

/** Services are framework-owned too: fields are injected before onCreate. */
public class PingService extends Service {
  @Inject Clock clock;

  @Override
  public void onCreate() {
    Log.i(InjectDemoApp.TAG, "Service clock#" + clock.id());
  }

  @Override
  public IBinder onBind(Intent intent) {
    return null;
  }
}
