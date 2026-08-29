// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon;

import javax.inject.Singleton;
import picodroid.content.SharedPreferences;
import picodroid.di.Module;
import picodroid.di.Provides;

/**
 * Bindings for SDK types, which cannot carry an {@code @Inject} constructor. Installed
 * automatically (single implicit component).
 */
@Module
public final class EnvModule {
  private EnvModule() {}

  /** The app's preferences file, opened once. */
  @Provides
  @Singleton
  static SharedPreferences providePrefs() {
    return SharedPreferences.open(EnvApp.PREFS_NAME);
  }
}
