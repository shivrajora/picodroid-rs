// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Inject
import picodroid.app.Activity

/**
 * Superclass members are injected first, before the leaf's own. Kotlin classes are final by
 * default, so a base Activity must be `abstract` (or `open`).
 */
abstract class BaseActivity : Activity() {
    @Inject lateinit var clock: Clock
}
