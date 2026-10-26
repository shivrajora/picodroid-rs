// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import javax.inject.Singleton;
import picodroid.content.Context;
import picodroid.content.SharedPreferences;
import picodroid.di.Module;
import picodroid.di.Provides;

/**
 * Bindings for the SDK types the graph needs, which cannot carry an {@code @Inject} constructor of
 * their own. Installed automatically — there is a single implicit component.
 */
@Module
public final class ClockModule {
  private ClockModule() {}

  /** The app's preferences file, shared by every screen and the Service. */
  @Provides
  @Singleton
  static SharedPreferences providePrefs() {
    return ClockApp.appContext().getSharedPreferences(ClockApp.PREFS_NAME, Context.MODE_PRIVATE);
  }
}
