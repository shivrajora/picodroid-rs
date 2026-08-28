// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * Enums: constructor parameters, `entries` (`EnumEntriesKt.enumEntries` in `<clinit>`, a `List`
 * over the constants), `values()`, `ordinal`/`name`, methods, exhaustive `when`, ordering.
 */
object EnumDemo {
    private const val TAG = "EnumKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    enum class Kind(val label: String, val code: Int) {
        TEMP("t", 1),
        HUM("h", 2),
        PRESS("p", 3);

        fun isWet(): Boolean = this == HUM

        companion object {
            fun fromLabel(l: String): Kind? {
                for (k in entries) if (k.label == l) return k
                return null
            }
        }
    }

    enum class Plain {
        A,
        B,
    }

    private fun letter(k: Kind): Char =
        when (k) {
            Kind.TEMP -> 'T'
            Kind.HUM -> 'H'
            Kind.PRESS -> 'P'
        }

    fun run() {
        Log.i(TAG, "=== Enum Tests ===")

        check("entries size", Kind.entries.size == 3)
        check("entries get", Kind.entries[1] == Kind.HUM)
        var codes = 0
        for (k in Kind.entries) codes += k.code
        check("for over entries", codes == 6)
        check("entries contains", Kind.HUM in Kind.entries)
        check("entries indexOf", Kind.entries.indexOf(Kind.PRESS) == 2)
        check("entries not empty", !Kind.entries.isEmpty())
        check("entries firstOrNull", Kind.entries.firstOrNull { it.code == 2 } == Kind.HUM)
        val it = Kind.entries.iterator()
        check("entries iterator", it.hasNext() && it.next() == Kind.TEMP)
        var oob = false
        try {
            Log.i(TAG, "unreachable ${Kind.entries[5]}")
        } catch (e: IndexOutOfBoundsException) {
            oob = true
        }
        check("entries out of bounds", oob)

        check("values size", Kind.values().size == 3)
        check("ordinal", Kind.TEMP.ordinal == 0 && Kind.PRESS.ordinal == 2)
        check("name", Kind.PRESS.name == "PRESS")
        check("toString", Kind.HUM.toString() == "HUM")
        check("template", "k=${Kind.TEMP}" == "k=TEMP")
        check("ctor params", Kind.HUM.label == "h" && Kind.HUM.code == 2)
        check("enum method", Kind.HUM.isWet() && !Kind.TEMP.isWet())
        check("companion lookup", Kind.fromLabel("p") == Kind.PRESS && Kind.fromLabel("z") == null)
        check("exhaustive when", letter(Kind.TEMP) == 'T' && letter(Kind.PRESS) == 'P')
        check("compareTo", Kind.TEMP < Kind.HUM && Kind.PRESS > Kind.HUM)
        check("equals/identity", Kind.TEMP == Kind.entries[0] && Kind.TEMP === Kind.entries[0])
        check("hashCode stable", Kind.HUM.hashCode() == Kind.entries[1].hashCode())
        check("plain enum", Plain.entries.size == 2 && Plain.B.ordinal == 1)

        Check.done(TAG)
    }
}
