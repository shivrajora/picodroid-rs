// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import picodroid.di.Module
import picodroid.di.Provides

/**
 * The static-`@Provides` shape in Kotlin: a `@Module object` whose methods are `@JvmStatic`, so
 * kotlinc emits one `public static final` method and the processor never needs a module instance
 * (the object's constructor is private, which the instance path would reject). Without `@JvmStatic`
 * the method is an instance method and the build fails with "needs a non-private no-arg
 * constructor".
 */
@Module
object StaticModule {
    /** Interface binding with an injected dependency: called on every injection (unscoped). */
    @Provides
    @JvmStatic
    fun provideGreeting(clock: Clock): Greeting = Greeting { who -> "hi $who @clock#${clock.id}" }
}
