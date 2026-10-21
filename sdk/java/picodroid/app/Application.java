// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.ComponentCallbacks2;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.res.Configuration;

/**
 * The app's process-wide state and entry point, mirroring {@code android.app.Application}: the
 * framework constructs the manifest's subclass once, calls {@link #onCreate}, and keeps it for the
 * run. Its memory callbacks are where {@link Context#registerComponentCallbacks} fans out, as on
 * Android, so an override must call {@code super}.
 */
public class Application extends Context implements ComponentCallbacks2 {
  public Application() {
    Context.sApplication = this;
  }

  public void onCreate() {
    // Subclass overrides this
  }

  @Override
  public native void startActivity(Intent intent);

  /**
   * Memory is short: see {@link ComponentCallbacks2#onTrimMemory}. Fans out to the registered
   * callbacks.
   */
  @Override
  public void onTrimMemory(int level) {
    Context.fireRegisteredCallbacks(level);
  }

  /** Fans out to the registered callbacks. */
  @Override
  public void onLowMemory() {
    Context.fireRegisteredCallbacks(Context.LOW_MEMORY);
  }

  /** Never called on picodroid; see {@link ComponentCallbacks2#onConfigurationChanged}. */
  @Override
  public void onConfigurationChanged(Configuration newConfig) {
    // A board's configuration does not change while an app runs.
  }
}
