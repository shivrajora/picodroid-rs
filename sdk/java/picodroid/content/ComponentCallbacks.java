// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import picodroid.content.res.Configuration;

/**
 * The callbacks every application component shares, mirroring {@code
 * android.content.ComponentCallbacks}. {@link Context#registerComponentCallbacks} adds any
 * implementation to the set the framework notifies; {@code Activity}, {@code Service} and {@code
 * Application} are in it by construction.
 */
public interface ComponentCallbacks {
  /**
   * Declared for signature parity with Android. Never called: a board's configuration (screen size,
   * orientation, locale) does not change while an app runs.
   */
  void onConfigurationChanged(Configuration newConfig);

  /**
   * The framework is about to destroy a covered Activity to make room: release anything that can be
   * rebuilt. See {@link ComponentCallbacks2#onTrimMemory} for the earlier, gentler signal.
   */
  void onLowMemory();
}
