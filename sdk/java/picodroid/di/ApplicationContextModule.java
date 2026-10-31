// SPDX-License-Identifier: GPL-3.0-only
package picodroid.di;

import picodroid.app.Application;

/**
 * The framework's bindings in the injection graph — the picodroid counterpart of Hilt's {@code
 * ApplicationContextModule}: {@code picodroid.app.Application} and {@code @ApplicationContext
 * Context} both resolve to {@link #provideApplication}. The {@code @Inject} processor's generated
 * factories call it; an app injects one of the two instead of calling it. See {@code
 * docs/designs/inject-annotations-2026-08.md}.
 */
public final class ApplicationContextModule {
  private ApplicationContextModule() {}

  /**
   * The app's Application, also its application Context; a bare {@code Application} when the
   * manifest names none (Android always has one).
   */
  public static Application provideApplication() {
    return Application.currentApplication();
  }
}
