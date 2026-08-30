// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Singleton
import picodroid.di.Module
import picodroid.di.Provides

/**
 * The instance-`@Provides` shape: a `@Module class` with Kotlin's default public no-arg
 * constructor, created once, lazily, through the generated `InstanceModule_Factory`.
 */
@Module
class InstanceModule {
    private var banners = 0

    /** Scoped: one Banner per process, so the counter proves the method ran once. */
    @Provides
    @Singleton
    fun provideBanner(): Banner {
        banners++
        return Banner("banner#$banners")
    }
}
