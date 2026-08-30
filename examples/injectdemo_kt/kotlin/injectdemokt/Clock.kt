// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Inject
import javax.inject.Singleton

/** App-scoped: every injection site sees the one instance, so `id` is always 1. */
@Singleton
class Clock @Inject constructor() {
    val id: Int = ++clocksCreated
}
