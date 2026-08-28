// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/**
 * Null safety: `?.`, `?:`, `!!` (`Intrinsics.checkNotNull`), `lateinit`
 * (`UninitializedPropertyAccessException`), `as?` / `as` / `is`, `let`, `checkNotNull` /
 * `requireNotNull`, and nullable primitives.
 */
object NullSafetyDemo {
    private const val TAG = "NullSafetyKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private lateinit var late: String

    class Box(val v: Int?)

    private fun maybe(flag: Boolean): String? = if (flag) "yes" else null

    /** Hides the static type so `as?` / `as` are real runtime checks (no compile-time warnings). */
    private fun opaque(v: Any?): Any? = v

    fun run() {
        Log.i(TAG, "=== Null Safety Tests ===")

        val s = maybe(true)
        val n = maybe(false)
        check("safe call", s?.length == 3)
        check("safe call on null", n?.length == null)
        check("elvis", (n ?: "dflt") == "dflt")
        check("bang", s!!.length == 3)
        var npe = false
        try {
            Log.i(TAG, "unreachable ${n!!.length}")
        } catch (e: NullPointerException) {
            npe = true
        }
        check("!! throws NPE", npe)

        var uninit = false
        try {
            Log.i(TAG, "unreachable ${late.length}")
        } catch (e: UninitializedPropertyAccessException) {
            uninit = e.message == "lateinit property late has not been initialized"
        }
        check("lateinit read before init", uninit)
        late = "set"
        check("lateinit after init", late.length == 3)

        val any = opaque("str")
        check("as? success", (any as? String) == "str")
        check("as? failure", (any as? Int) == null)
        var cce = false
        try {
            Log.i(TAG, "unreachable ${any as Int}")
        } catch (e: ClassCastException) {
            cce = true
        }
        check("as throws CCE", cce)
        var npe2 = false
        try {
            Log.i(TAG, "unreachable ${opaque(null) as String}")
        } catch (e: NullPointerException) {
            npe2 = true
        }
        check("null as T throws NPE", npe2)
        check("is + smart cast", any is String && any.length == 3)
        check("let on null skipped", n?.let { it.length } == null)
        check("let on value", s?.let { it.length } == 3)
        check("template with null", "v=$n" == "v=null")
        check("String? plus (Intrinsics.stringPlus)", n + "x" == "nullx" && s + 1 == "yes1")

        val b: Int? = 5
        check("nullable Int arithmetic", b?.plus(1) == 6)
        val boxes = arrayOf(Box(1), Box(null), Box(3))
        var sum = 0
        for (bx in boxes) sum += bx.v ?: 0
        check("elvis over nullable Int fields", sum == 4)

        var ise = false
        try {
            checkNotNull(n) { "was null" }
        } catch (e: IllegalStateException) {
            ise = e.message == "was null"
        }
        check("checkNotNull throws ISE", ise)
        var iae = false
        try {
            requireNotNull(n) { "req" }
        } catch (e: IllegalArgumentException) {
            iae = e.message == "req"
        }
        check("requireNotNull throws IAE", iae)

        Check.done(TAG)
    }
}
