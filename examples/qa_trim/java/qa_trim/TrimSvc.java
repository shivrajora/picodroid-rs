// SPDX-License-Identifier: GPL-3.0-only
package qa_trim;

import picodroid.app.Service;
import picodroid.content.Intent;
import picodroid.os.IBinder;

/** A started Service that only listens for the memory signal. */
public class TrimSvc extends Service {
  @Override
  public IBinder onBind(Intent intent) {
    return null;
  }

  @Override
  public void onTrimMemory(int level) {
    T.log("svc:" + level);
  }

  @Override
  public void onLowMemory() {
    T.log("svc:low");
  }
}
