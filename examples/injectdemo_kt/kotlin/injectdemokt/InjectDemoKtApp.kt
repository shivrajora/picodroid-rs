// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Inject
import javax.inject.Provider
import picodroid.app.Application
import picodroid.content.Intent
import picodroid.di.ApplicationComponent
import picodroid.di.Lazy
import picodroid.util.Log

/**
 * The Kotlin twin of `examples/injectdemo`: `@Inject` / `@Singleton` end to end, with the javac
 * processor run through kapt over kotlinc's stubs. The framework injects this Application's fields
 * before `onCreate`, then every Activity and Service it starts gets the same treatment. Assertions
 * ride on the log lines (see scripts/hil-tests.conf).
 *
 * Kotlin shapes the processor accepts: `@Inject lateinit var` (a public backing field — a plain
 * `var`/`val` has a private one and is rejected), `@Inject constructor`, `@Singleton` on the class,
 * `@Module object` + `@JvmStatic @Provides` (static path), `@Module class` for instance
 * `@Provides`. Never put `@Provides` in a `companion object` (kotlinc emits the method twice) and
 * import `picodroid.di.Lazy`, not `kotlin.Lazy`.
 */
class InjectDemoKtApp : Application() {
    @Inject lateinit var clock: Clock
    @Inject lateinit var greeter: Greeter
    @Inject lateinit var legacy: LegacyComponent

    /** A fresh (unscoped) Greeter per get(). */
    @Inject lateinit var greeters: Provider<Greeter>

    /**
     * Deferred until first get(), then memoized — and a @Singleton's Lazy is the shared instance.
     */
    @Inject lateinit var lazyClock: Lazy<Clock>

    /**
     * Module-provided bindings: an interface (unscoped) and a value type (@Singleton @Provides).
     */
    @Inject lateinit var greeting: Greeting

    @Inject lateinit var banner: Banner
    @Inject lateinit var banners: Provider<Banner>

    override fun onCreate() {
        appGreeter = greeter
        Log.i(TAG, "app clock#${clock.id} legacy=${ApplicationComponent.current() === legacy}")

        // A generated class referenced from Kotlin: kapt's output dir is a source root of
        // compileKotlin, exactly as it is for Dagger's generated components.
        val m = Message_Factory.get()
        Log.i(
            TAG,
            "Message fields=${if (m.fieldsOk()) "ok" else "BAD"} method=${if (m.methodOk()) "ok" else "BAD"}",
        )

        val first = greeters.get()
        val second = greeters.get()
        val lazyOnce = lazyClock.get()
        val lazyTwice = lazyClock.get()
        Log.i(
            TAG,
            "Provider fresh=${first !== second} Lazy clock#${lazyOnce.id} memo=${lazyOnce === lazyTwice}",
        )

        Log.i(
            TAG,
            "Module iface=${greeting.greet("x")} banner=${banner.text} singleton=${banners.get() === banner}",
        )

        startService(Intent(PingService::class.java))
        startActivity(Intent(HomeActivity::class.java))
    }
}
