// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

/** An interface binding: no @Inject constructor possible, so StaticModule provides it. */
fun interface Greeting {
    fun greet(who: String): String
}
