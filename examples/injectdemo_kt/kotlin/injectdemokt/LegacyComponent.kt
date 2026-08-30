// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Inject
import javax.inject.Singleton
import picodroid.di.ApplicationComponent

/**
 * Coexistence with the manual-DI shape: a hand-written [ApplicationComponent] subclass can itself
 * be a `@Singleton` with an `@Inject` constructor, so it is injectable while
 * `ApplicationComponent.current()` keeps working for legacy call sites.
 */
@Singleton class LegacyComponent @Inject constructor() : ApplicationComponent()
