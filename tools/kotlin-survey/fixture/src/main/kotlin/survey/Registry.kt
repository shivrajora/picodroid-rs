// SPDX-License-Identifier: GPL-3.0-only
package survey

import picodroid.content.SharedPreferences
import picodroid.util.Log

/** `object` singleton: INSTANCE init, `const val`, `@JvmField`, `@JvmStatic`. */
object Registry {
    const val MAX = 8
    val created = mutableListOf<String>()

    @JvmField val version = 3

    @JvmStatic
    fun register(n: String) {
        if (created.size < MAX) created += n
    }
}

/**
 * ThresholdConfig shape: companion with `const`/`val`/`@JvmField`/`@JvmStatic`, SharedPreferences
 * load/save.
 */
class Config(val limit: Int, val name: String) {
    companion object {
        const val KEY = "limit"
        val DEFAULT = Config(60, "default")

        @JvmField val NAMES = arrayOf("a", "b")

        @JvmStatic
        fun load(p: SharedPreferences): Config =
            Config(p.getInt(KEY, DEFAULT.limit), p.getString("name", DEFAULT.name))

        fun save(p: SharedPreferences, c: Config) {
            p.edit().putInt(KEY, c.limit).putString("name", c.name).commit()
        }
    }
}

/** `Array<String>` vs `IntArray`, and the same SAM as an anonymous object vs an indy lambda. */
object ArrayShapes {
    val strings: Array<String> = arrayOf("x", "y")
    val ints: IntArray = intArrayOf(1, 2, 3)
    val anon: Runnable =
        object : Runnable {
            override fun run() {
                Log.d("Registry", "anon")
            }
        }
    val indy: Runnable = Runnable { Log.d("Registry", "indy") }

    fun total(): Int =
        strings.size + ints.size + Registry.version + Registry.MAX + Config.NAMES.size
}
