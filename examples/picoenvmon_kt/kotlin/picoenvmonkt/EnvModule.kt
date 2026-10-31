// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt

import javax.inject.Singleton
import picodroid.content.Context
import picodroid.content.SharedPreferences
import picodroid.di.ApplicationContext
import picodroid.di.Module
import picodroid.di.Provides

/**
 * Bindings for SDK types, which cannot carry an `@Inject` constructor. Installed automatically
 * (single implicit component). A `@Module object` with `@JvmStatic` providers is the Kotlin form of
 * Java's `final class` + `static` method: the processor calls it statically and never needs an
 * instance.
 */
@Module
object EnvModule {
    /** The app's preferences file, behind `ThresholdConfig`; opened once. */
    @Provides
    @Singleton
    @JvmStatic
    fun providePrefs(@ApplicationContext context: Context): SharedPreferences =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
}
