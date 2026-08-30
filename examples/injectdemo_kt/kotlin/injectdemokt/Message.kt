// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Inject

/**
 * All three injection kinds on one plain class: constructor, field, and method. Built from ordinary
 * code through the generated `Message_Factory.get()` — the entry point for pulling a graph object
 * outside a framework-owned component.
 */
class Message @Inject constructor(private val ctorClock: Clock) {
    @Inject lateinit var clock: Clock

    private var viaMethod: Greeter? = null

    @Inject
    fun setGreeter(greeter: Greeter) {
        viaMethod = greeter
    }

    fun fieldsOk(): Boolean = clock === ctorClock

    fun methodOk(): Boolean = viaMethod?.clock === clock
}
