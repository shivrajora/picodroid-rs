// SPDX-License-Identifier: GPL-3.0-only
package qa_trim;

import picodroid.app.Application;
import picodroid.content.ComponentCallbacks2;
import picodroid.content.Intent;
import picodroid.content.res.Configuration;

/**
 * QA 2026-10-20: {@code onTrimMemory}. The Application registers an extra callback and starts a
 * Service and Home; Home starts Fill, which grows a hidden view cache until the framework reports
 * the LVGL pool short, then checks that {@code TRIM_MEMORY_RUNNING_CRITICAL} reached every
 * component in Android's order, drops the cache, and that the next launch reclaims nothing.
 */
public class QaTrimApp extends Application {
  @Override
  public void onCreate() {
    registerComponentCallbacks(
        new ComponentCallbacks2() {
          @Override
          public void onTrimMemory(int level) {
            T.log("extra:" + level);
          }

          @Override
          public void onLowMemory() {
            T.log("extra:low");
          }

          @Override
          public void onConfigurationChanged(Configuration newConfig) {
            T.log("extra:config");
          }
        });
    startService(new Intent(TrimSvc.class));
    startActivity(new Intent(ActHome.class));
  }

  @Override
  public void onTrimMemory(int level) {
    T.log("app:" + level);
    super.onTrimMemory(level);
  }

  @Override
  public void onLowMemory() {
    T.log("app:low");
    super.onLowMemory();
  }
}
