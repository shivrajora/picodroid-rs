// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Singleton;
import picodroid.di.Module;
import picodroid.di.Provides;

/**
 * Bindings for types that cannot carry an {@code @Inject} constructor. Every {@code @Module} in the
 * app is installed automatically. {@code provideGreeting} is static (no module instance needed);
 * {@code provideBanner} is an instance method, so the module is created once, lazily, through the
 * generated {@code DemoModule_Factory}.
 */
@Module
public class DemoModule {
  private int banners;

  /** Interface binding with an injected dependency: called on every injection (unscoped). */
  @Provides
  static Greeting provideGreeting(final Clock clock) {
    return new Greeting() {
      @Override
      public String greet(String who) {
        return "hi " + who + " @clock#" + clock.id();
      }
    };
  }

  /** Scoped: one Banner per process, so the counter proves the method ran once. */
  @Provides
  @Singleton
  Banner provideBanner() {
    banners++;
    return new Banner("banner#" + banners);
  }
}
