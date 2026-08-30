// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Inject

/** Unscoped: a fresh Greeter per injection site, all sharing the singleton Clock. */
class Greeter @Inject constructor(val clock: Clock) {
    fun greet(who: String): String = "hello $who @clock#${clock.id}"
}
