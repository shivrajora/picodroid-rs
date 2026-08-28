// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * Data classes: generated `toString`/`equals`/`hashCode`/`copy`/`componentN` over Int, Float, Long,
 * Boolean, Char, Double, String and nullable fields; string templates with boxed and object parts;
 * `String.format`.
 */
object DataClassDemo {
    private const val TAG = "DataClassKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    data class Reading(val type: Int, val value: Float, val ts: Long)

    data class Named(val name: String, val n: Int)

    data class Flags(val on: Boolean, val ch: Char, val d: Double, val extra: String?)

    private fun opaque(v: Any?): Any? = v

    fun run() {
        Log.i(TAG, "=== Data Class Tests ===")

        val r = Reading(1, 2.5f, 3L)
        check("toString", r.toString() == "Reading(type=1, value=2.5, ts=3)")
        check("equals", r == Reading(1, 2.5f, 3L))
        check("not equals", r != Reading(2, 2.5f, 3L))
        check("hashCode consistent", r.hashCode() == Reading(1, 2.5f, 3L).hashCode())
        check("hashCode differs", r.hashCode() != Reading(1, 2.5f, 4L).hashCode())
        check("equals null", !r.equals(opaque(null)))
        check("equals other type", !r.equals(opaque("x")))
        val c = r.copy(value = 0f)
        check("copy", c.type == 1 && c.value == 0f && c.ts == 3L)
        check("copy toString", c.toString() == "Reading(type=1, value=0.0, ts=3)")
        val (t, v, ts) = r
        check("destructuring", t == 1 && v == 2.5f && ts == 3L)

        val a = Named("a", 1)
        check("String field equals", a == Named("a", 1))
        check("String field not equals", a != Named("b", 1))
        check("String field hashCode", a.hashCode() == Named("a", 1).hashCode())
        check("String field toString", a.toString() == "Named(name=a, n=1)")

        val f = Flags(true, 'z', 1.5, null)
        check(
            "bool/char/double/null toString",
            f.toString() == "Flags(on=true, ch=z, d=1.5, extra=null)",
        )
        check("nullable field equals", f == Flags(true, 'z', 1.5, null))
        check("nullable field differs", f != Flags(true, 'z', 1.5, "e"))

        check("template object part", "r=$r" == "r=Reading(type=1, value=2.5, ts=3)")
        val m: Int? = 7
        check("template Int? part", "v=$m" == "v=7")
        check("template float part", "${r.value}" == "2.5")
        check("template long part", "${r.ts}" == "3")
        check("template mixed", "${a.name}:${a.n}:${f.on}:${f.ch}" == "a:1:true:z")
        check("String.format", "%d-%s".format(4, "x") == "4-x")
        check(
            "Int.compareTo (Intrinsics.compare)",
            3.compareTo(4) < 0 && 4.compareTo(4) == 0 && 5.compareTo(4) > 0,
        )
        check(
            "Long.compareTo (Intrinsics.compare)",
            5L.compareTo(2L) > 0 && (-1L).compareTo(2L) < 0,
        )
        check("String.format float", "%.1f".format(2.25f) == "2.2" || "%.1f".format(2.25f) == "2.3")

        Check.done(TAG)
    }
}
