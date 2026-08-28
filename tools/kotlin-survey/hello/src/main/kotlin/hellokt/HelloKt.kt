// SPDX-License-Identifier: GPL-3.0-only
package hellokt

import picodroid.app.Application
import picodroid.util.Log

/**
 * The hello-kotlin-on-sim milestone: exactly one class, no companion, no
 * top-level functions (a file facade would also be named `HelloKt`), and
 * nothing that needs the Kotlin stdlib at run time. The `${21 * 2}` template
 * folds to `42` at compile time; the only stdlib trace left is the `@Metadata`
 * annotation, which pico-jvm skips by length.
 */
class HelloKt : Application() {
    override fun onCreate() {
        Log.i("HelloKt", "hi from kotlin ${21 * 2}")
    }
}
