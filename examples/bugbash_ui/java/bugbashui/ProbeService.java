// SPDX-License-Identifier: GPL-3.0-only
package bugbashui;

import picodroid.app.Service;
import picodroid.content.Intent;
import picodroid.os.IBinder;
import picodroid.util.Log;

public class ProbeService extends Service {
  static boolean destroyed = false;

  private final IBinder binder = new IBinder() {};

  @Override
  public IBinder onBind(Intent intent) {
    Log.i("BugBashUi", "ProbeService.onBind");
    return binder;
  }

  @Override
  public void onDestroy() {
    destroyed = true;
    Log.i("BugBashUi", "ProbeService.onDestroy");
  }
}
