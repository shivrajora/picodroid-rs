// SPDX-License-Identifier: GPL-3.0-only
package gcstresskt

/** App-defined SAM so lambda capture goes through an `invokedynamic` proxy the app owns. */
fun interface Supplier {
    fun get(): Int
}
