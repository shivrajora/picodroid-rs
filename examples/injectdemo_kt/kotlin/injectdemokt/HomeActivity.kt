// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Inject
import picodroid.content.Intent
import picodroid.util.Log
import picodroid.widget.TextView

class HomeActivity : BaseActivity() {
    @Inject lateinit var greeter: Greeter

    private var pushed = false

    override fun onCreate() {
        Log.i(
            TAG,
            "Home clock#${clock.id} same=${clock === greeter.clock} fresh=${greeter !== appGreeter}",
        )
        getDisplay()
        val text = TextView()
        text.setText(greeter.greet("home"))
        setContentView(text)
        if (!pushed) {
            pushed = true
            startActivity(Intent(DetailActivity::class.java))
        }
    }
}
