// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon;

import javax.inject.Singleton;
import picodroid.content.Context;
import picodroid.content.SharedPreferences;
import picodroid.di.ApplicationContext;
import picodroid.di.Module;
import picodroid.di.Provides;

/**
 * Bindings for SDK types, which cannot carry an {@code @Inject} constructor. Installed
 * automatically (single implicit component).
 */
@Module
public final class EnvModule {
  private EnvModule() {}

  /** The app's preferences file, behind {@code ThresholdConfig}; opened once. */
  @Provides
  @Singleton
  static SharedPreferences providePrefs(@ApplicationContext Context context) {
    return context.getSharedPreferences(EnvApp.PREFS_NAME, Context.MODE_PRIVATE);
  }
}
