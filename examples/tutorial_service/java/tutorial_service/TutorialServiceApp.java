// SPDX-License-Identifier: GPL-3.0-only
package tutorial_service;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * App entry point. The framework instantiates the Application named in PicodroidManifest.xml and
 * calls onCreate once at boot.
 *
 * <p>Order matters here: we {@code startService} the {@link UptimeLogService} <em>before</em>
 * launching the first Activity. Starting it from the Application (rather than from an Activity) is
 * what makes it outlive any single screen — it keeps sampling on its background Thread while the
 * user navigates, so the {@link LogViewerActivity} always finds an accumulated ring buffer to read.
 */
public class TutorialServiceApp extends Application {
  @Override
  public void onCreate() {
    Log.i("TutorialServiceApp", "starting UptimeLogService");
    startService(new Intent(UptimeLogService.class));
    startActivity(new Intent(HomeActivity.class));
  }
}
