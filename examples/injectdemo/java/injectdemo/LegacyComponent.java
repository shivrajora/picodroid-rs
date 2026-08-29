// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Inject;
import javax.inject.Singleton;
import picodroid.di.ApplicationComponent;

/**
 * Coexistence with the manual-DI shape: a hand-written {@link ApplicationComponent} subclass can
 * itself be a {@code @Singleton} with an {@code @Inject} constructor, so it is injectable while
 * {@code ApplicationComponent.current()} keeps working for legacy call sites.
 */
@Singleton
public class LegacyComponent extends ApplicationComponent {
  @Inject
  public LegacyComponent() {
    super();
  }

  public String tag() {
    return InjectDemoApp.TAG;
  }
}
