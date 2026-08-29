// SPDX-License-Identifier: GPL-3.0-only
package langsuitekt

import picodroid.util.Log

/** `synchronized(lock) { }` (monitorenter/monitorexit inline), `@Synchronized`, `@Volatile`. */
object SyncDemo {
    private const val TAG = "SyncKt"

    private fun check(name: String, ok: Boolean) = Check.check(TAG, name, ok)

    private fun opaque(v: Any?): Any? = v

    private val lock = Any()
    private var counter = 0
    @Volatile private var flag = false

    private class Account(private var balance: Int) {
        private val guard = Any()

        fun deposit(n: Int): Int =
            synchronized(guard) {
                balance += n
                balance
            }

        @Synchronized
        fun withdraw(n: Int): Boolean {
            if (balance < n) return false
            balance -= n
            return true
        }

        fun peek() = synchronized(this) { balance }
    }

    private fun earlyReturn(b: Boolean): Int {
        synchronized(lock) { if (b) return 1 }
        return 2
    }

    fun run() {
        Log.i(TAG, "=== Sync Tests ===")

        check("synchronized returns value", synchronized(lock) { 41 + 1 } == 42)
        synchronized(lock) { counter++ }
        check("synchronized side effect", counter == 1)
        check(
            "nested synchronized same lock",
            synchronized(lock) {
                synchronized(lock) {
                    counter += 1
                    counter
                }
            } == 2,
        )
        check("nested different locks", synchronized(lock) { synchronized(Any()) { "ok" } } == "ok")
        var caught = false
        try {
            synchronized(lock) { throw IllegalStateException("inside") }
        } catch (e: IllegalStateException) {
            caught = e.message == "inside"
        }
        check("exception unwinds monitor", caught)
        check("lock reusable after exception", synchronized(lock) { "again" } == "again")
        val acct = Account(10)
        check("synchronized in class", acct.deposit(5) == 15 && acct.peek() == 15)
        check("@Synchronized method", !acct.withdraw(20) && acct.withdraw(15) && acct.peek() == 0)
        flag = true
        check("@Volatile field", flag)
        check(
            "synchronized inside lambda",
            listOf(1, 2, 3).map { synchronized(lock) { it * 2 } }.sum() == 12,
        )
        check("early return inside synchronized", earlyReturn(true) == 1 && earlyReturn(false) == 2)
        check(
            "synchronized loop",
            run {
                var s = 0
                for (i in 1..5) synchronized(lock) { s += i }
                s == 15
            },
        )
        check(
            "synchronized with null-safe body",
            synchronized(lock) { (opaque(null) as String?)?.length ?: -1 } == -1,
        )
        check("synchronized on object singleton", synchronized(SyncDemo) { counter } == 2)
        check(
            "synchronized returning Unit",
            synchronized(lock) { counter = 0 } == Unit && counter == 0,
        )

        Check.done(TAG)
    }
}
